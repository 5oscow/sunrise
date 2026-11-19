//! CRC-32/ISO-HDLC AVX-512 folding, adapted from crc32fast 1.5.2
//! (https://github.com/srijs/rust-crc32fast). The final fold is a balanced
//! reduction rather than a serial chain, and crc32fast handles short tails.

// MIT License
//
// Copyright (c) 2018 Sam Rijs, Alex Crichton and contributors
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::{arch::x86_64::*, sync::OnceLock};

const STRIDE: usize = 256;
const MIN_WIDE_BYTES: usize = 2048;

pub(super) struct Hasher(u32);

impl Hasher {
    pub(super) fn new() -> Option<Self> {
        static WIDE: OnceLock<bool> = OnceLock::new();
        WIDE.get_or_init(|| {
            is_x86_feature_detected!("avx512f")
                && is_x86_feature_detected!("vpclmulqdq")
                && is_x86_feature_detected!("pclmulqdq")
        })
        .then_some(Self(0))
    }

    pub(super) fn update(&mut self, bytes: &[u8]) {
        let mut tail = bytes;
        if bytes.len() >= MIN_WIDE_BYTES {
            let (blocks, remainder) = bytes.as_chunks::<STRIDE>();
            let (first, rest) = blocks
                .split_first()
                .expect("wide CRC has at least one block");
            // SAFETY: a Hasher is constructed only after detecting the features
            // used by fold; first and rest contain only complete 256-byte blocks.
            self.0 = unsafe { fold(self.0, first, rest) };
            tail = remainder;
        }
        if !tail.is_empty() {
            let mut digest = crc32fast::Hasher::new_with_initial(self.0);
            digest.update(tail);
            self.0 = digest.finalize();
        }
    }

    pub(super) fn finalize(self) -> u32 {
        self.0
    }
}

// Fold coefficients for stride D (in bits): reflected x^(D±32) mod P.
const K_16: (i64, i64) = (0x0ccaa009e, 0x1751997d0);
const K_32: (i64, i64) = (0x15a546366, 0x0f1da05aa);
const K_64: (i64, i64) = (0x1c6e41596, 0x154442bd4);
const K_128: (i64, i64) = (0x14a7fe880, 0x1e88ef372);
const K_256: (i64, i64) = (0x1322d1430, 0x11542778a);

#[inline]
#[target_feature(enable = "avx512f")]
fn load<const OFFSET: usize>(block: &[u8; STRIDE]) -> __m512i {
    const { assert!(OFFSET <= STRIDE - 64) };
    // SAFETY: the specified offset leaves at least 64 readable bytes in the
    // block; loadu does not require alignment. AVX-512F is enabled here.
    unsafe { _mm512_loadu_si512(block.as_ptr().add(OFFSET).cast()) }
}

#[inline]
#[target_feature(enable = "avx512f")]
fn broadcast(k: (i64, i64)) -> __m512i {
    let (hi, lo) = k;
    _mm512_set_epi64(hi, lo, hi, lo, hi, lo, hi, lo)
}

#[inline]
#[target_feature(enable = "avx512f,vpclmulqdq")]
fn fold_wide(a: __m512i, b: __m512i, k: __m512i) -> __m512i {
    _mm512_ternarylogic_epi64(
        _mm512_clmulepi64_epi128(a, k, 0x00),
        _mm512_clmulepi64_epi128(a, k, 0x11),
        b,
        0x96, // XOR of three inputs.
    )
}

#[inline]
#[target_feature(enable = "pclmulqdq")]
fn fold_narrow(a: __m128i, b: __m128i, k: __m128i) -> __m128i {
    _mm_xor_si128(
        _mm_xor_si128(
            _mm_clmulepi64_si128(a, k, 0x00),
            _mm_clmulepi64_si128(a, k, 0x11),
        ),
        b,
    )
}

#[target_feature(enable = "avx512f,vpclmulqdq,pclmulqdq")]
fn fold(crc: u32, first: &[u8; STRIDE], rest: &[[u8; STRIDE]]) -> u32 {
    let initial = _mm512_zextsi128_si512(_mm_cvtsi32_si128(!crc as i32));
    let mut a = _mm512_xor_si512(load::<0>(first), initial);
    let mut b = load::<64>(first);
    let mut c = load::<128>(first);
    let mut d = load::<192>(first);
    let step = broadcast(K_256);
    for block in rest {
        a = fold_wide(a, load::<0>(block), step);
        b = fold_wide(b, load::<64>(block), step);
        c = fold_wide(c, load::<128>(block), step);
        d = fold_wide(d, load::<192>(block), step);
    }

    // Merge equal-distance groups before narrowing registers. In both the
    // wide and narrow halves of the tree, preserve the original byte order.
    let k128 = broadcast(K_128);
    let even = fold_wide(a, c, k128);
    let odd = fold_wide(b, d, k128);
    let lanes = fold_wide(even, odd, broadcast(K_64));
    let k32 = _mm_set_epi64x(K_32.0, K_32.1);
    let even = fold_narrow(
        _mm512_castsi512_si128(lanes),
        _mm512_extracti32x4_epi32(lanes, 2),
        k32,
    );
    let odd = fold_narrow(
        _mm512_extracti32x4_epi32(lanes, 1),
        _mm512_extracti32x4_epi32(lanes, 3),
        k32,
    );
    let k16 = _mm_set_epi64x(K_16.0, K_16.1);
    let x = fold_narrow(even, odd, k16);

    // Reduce 128 -> 96 -> 64 bits, then use a reflected Barrett reduction
    // modulo the ISO-HDLC polynomial to obtain the 32-bit CRC.
    let low32 = _mm_set_epi32(0, 0, 0, -1);
    let x = _mm_xor_si128(_mm_clmulepi64_si128(x, k16, 0x10), _mm_srli_si128(x, 8));
    let x = _mm_xor_si128(
        _mm_clmulepi64_si128(
            _mm_and_si128(x, low32),
            _mm_set_epi64x(0, 0x163cd6124),
            0x00,
        ),
        _mm_srli_si128(x, 4),
    );
    let poly = _mm_set_epi64x(0x1f7011641, 0x1db710641);
    let t = _mm_clmulepi64_si128(_mm_and_si128(x, low32), poly, 0x10);
    let t = _mm_clmulepi64_si128(_mm_and_si128(t, low32), poly, 0x00);
    !(_mm_cvtsi128_si32(_mm_srli_si128(_mm_xor_si128(x, t), 4)) as u32)
}

#!/usr/bin/env python3
"""Spike for sunrise#1279: generates a throwaway guest crate whose host calls are per-method named imports.

usage: gen_guest.py OUT_DIR VARIANT [N]

VARIANT:
  dyn        one trait of N methods, reached as `&dyn Api` from a `Box<dyn Listener>`; the mod calls m0 only
  static     the same trait, reached through a generic `fn run<H: Api>(&H)`; the mod calls m0 only
  concrete   the same trait, reached on the concrete `Guest`; the mod calls m0 only
  dyn_one    N methods split into traits of 5; only `&dyn T0` is ever created; the mod calls T0::m0
  dyn_all    the same split, every trait coerced to a `&dyn`; the mod calls T0::m0 only
  names      three single imports with 80, 1000 and 20000 byte names (question 1)
"""
import hashlib
import os
import sys

out, variant = sys.argv[1], sys.argv[2]
INL = ""
if variant.endswith("_inl"):  # the impl methods are #[inline]: codegen'd only in the crate that uses them
    variant, INL = variant[:-4], "#[inline] "
n = int(sys.argv[3]) if len(sys.argv) > 3 else 50


def hexhash(i):
    return hashlib.sha256(f"m{i}".encode()).hexdigest()[:32]


def call_name(i, owner="ServerHost"):
    return f"call:{owner}::method_{i}#{hexhash(i)}"


def method_body(i, owner="ServerHost"):
    # The import is declared in a block local to the method, as the generator would emit it.
    return (
        f'#[link(wasm_import_module = "sunrise")] unsafe extern "C" {{ '
        f'#[link_name = "{call_name(i, owner)}"] fn imp(a: u32) -> u32; }} '
        f"unsafe {{ imp(a) }}"
    )


API_HEAD = "#![allow(unused)]\npub struct Guest;\n"
EXPORTS = """
#[unsafe(no_mangle)] pub extern "C" fn sunrise_layout() -> u32 { 0 }
#[unsafe(no_mangle)] pub extern "C" fn sunrise_in(a: u32) -> u32 { a }
"""
api = ""   # lives in the api crate (the mod API: traits, Guest, the entry point of dyn variants)
mod = ""   # lives in the mod crate (the listener the mod writes)
USES = "use spike_api::*;\n"

if variant in ("dyn", "static", "concrete"):
    methods = "\n".join(f"    fn m{i}(&self, a: u32) -> u32;" for i in range(n))
    impls = "\n".join(
        f"    {INL}fn m{i}(&self, a: u32) -> u32 {{ {method_body(i)} }}" for i in range(n)
    )
    api = f"pub trait Api {{\n{methods}\n}}\nimpl Api for Guest {{\n{impls}\n}}\n"
    if variant == "dyn":
        # The game-side shape: the API crate's entry reaches the mod's listener through an opaque
        # `Box<dyn Listener>` factory, so nothing is inlined across the crates.
        api += """
pub trait Listener: Send + Sync { fn on(&self, host: &dyn Api, a: u32) -> u32; }
unsafe extern "Rust" { safe fn __spike_listener() -> Box<dyn Listener>; }
static LISTENER: std::sync::OnceLock<Box<dyn Listener>> = std::sync::OnceLock::new();
#[unsafe(no_mangle)] pub extern "C" fn sunrise_hook(a: u32) -> u32 {
    LISTENER.get_or_init(__spike_listener).on(&Guest, a)
}
"""
        mod = """
struct Mine;
impl Listener for Mine { fn on(&self, host: &dyn Api, a: u32) -> u32 { host.m0(a) } }
#[unsafe(no_mangle)] pub fn __spike_listener() -> Box<dyn Listener> { Box::new(Mine) }
"""
    elif variant == "static":
        api += "pub trait Listener { fn on<H: Api>(&self, host: &H, a: u32) -> u32; }\n"
        mod = """
struct Mine;
impl Listener for Mine { fn on<H: Api>(&self, host: &H, a: u32) -> u32 { host.m0(a) } }
#[unsafe(no_mangle)] pub extern "C" fn sunrise_hook(a: u32) -> u32 { Mine.on(&Guest, a) }
"""
    else:
        mod = """
#[unsafe(no_mangle)] pub extern "C" fn sunrise_hook(a: u32) -> u32 { Guest.m0(a) }
"""
elif variant in ("dyn_one", "dyn_all"):
    traits = n // 5
    text = ""
    for t in range(traits):
        text += f"pub trait T{t} {{\n"
        text += "\n".join(f"    fn m{k}(&self, a: u32) -> u32;" for k in range(5))
        text += f"\n}}\nimpl T{t} for Guest {{\n"
        text += "\n".join(
            f"    {INL}fn m{k}(&self, a: u32) -> u32 {{ {method_body(t * 5 + k, f'T{t}')} }}"
            for k in range(5)
        )
        text += "\n}\n"
    api = text
    if variant == "dyn_one":
        args, coerce, params = "host: &dyn T0", "&Guest", "host"
    else:
        args = ", ".join(f"host{t}: &dyn T{t}" for t in range(traits))
        coerce = ", ".join("&Guest" for _ in range(traits))
        params = "host0"
    api += f"""
pub trait Listener: Send + Sync {{ fn on(&self, {args}, a: u32) -> u32; }}
unsafe extern "Rust" {{ safe fn __spike_listener() -> Box<dyn Listener>; }}
static LISTENER: std::sync::OnceLock<Box<dyn Listener>> = std::sync::OnceLock::new();
#[unsafe(no_mangle)] pub extern "C" fn sunrise_hook(a: u32) -> u32 {{
    LISTENER.get_or_init(__spike_listener).on({coerce}, a)
}}
"""
    first = "host" if variant == "dyn_one" else "host0"
    mod = f"""
struct Mine;
impl Listener for Mine {{ fn on(&self, {args}, a: u32) -> u32 {{ {first}.m0(a) }} }}
#[unsafe(no_mangle)] pub fn __spike_listener() -> Box<dyn Listener> {{ Box::new(Mine) }}
"""
elif variant == "dead":
    # concrete dispatch of one method, plus unused functions that each own an import: pub and private in the mod
    # crate, pub in the api crate, and a generic that is never instantiated.
    def own(tag):
        return (f'#[link(wasm_import_module = "sunrise")] unsafe extern "C" {{ '
                f'#[link_name = "call:Dead::{tag}#{hexhash(tag)}"] fn imp(a: u32) -> u32; }} unsafe {{ imp(a) }}')
    api = (f"pub trait Api {{ fn m0(&self, a: u32) -> u32; }}\n"
           f"impl Api for Guest {{ fn m0(&self, a: u32) -> u32 {{ {method_body(0)} }} }}\n"
           f"pub fn api_unused_pub(a: u32) -> u32 {{ {own('api_pub')} }}\n")
    mod = (f"pub fn mod_unused_pub(a: u32) -> u32 {{ {own('mod_pub')} }}\n"
           f"fn mod_unused_private(a: u32) -> u32 {{ {own('mod_private')} }}\n"
           f"pub fn mod_unused_generic<T>(a: u32) -> u32 {{ {own('mod_generic')} }}\n"
           '#[unsafe(no_mangle)] pub extern "C" fn sunrise_hook(a: u32) -> u32 { Guest.m0(a) }\n')
elif variant == "names":
    calls = []
    for k, size in enumerate((80, 1000, 20000)):
        prefix = "call:ServerHost::long_method#"
        name = prefix + (hexhash(k) * 1000)[: size - len(prefix)]
        assert len(name) == size
        mod += (
            f'fn long{k}(a: u32) -> u32 {{ #[link(wasm_import_module = "sunrise")] unsafe extern "C" {{ '
            f'#[link_name = "{name}"] fn imp(a: u32) -> u32; }} unsafe {{ imp(a) }} }}\n'
        )
        calls.append(f"long{k}(a)")
    mod += (
        '#[unsafe(no_mangle)] pub extern "C" fn sunrise_hook(a: u32) -> u32 { '
        + " + ".join(calls)
        + " }\n"
    )
else:
    raise SystemExit(f"unknown variant {variant}")

name = f"spike_{variant}{'_inl' if INL else ''}".replace("_", "-")
os.makedirs(os.path.join(out, "src"), exist_ok=True)
os.makedirs(os.path.join(out, "api/src"), exist_ok=True)
with open(os.path.join(out, "api/src/lib.rs"), "w") as f:
    f.write(API_HEAD + api)
with open(os.path.join(out, "api/Cargo.toml"), "w") as f:
    f.write('[package]\nname = "spike-api"\nversion = "0.1.0"\nedition = "2024"\npublish = false\n')
with open(os.path.join(out, "src/lib.rs"), "w") as f:
    f.write("#![allow(unused)]\n" + USES + mod + EXPORTS)
with open(os.path.join(out, "Cargo.toml"), "w") as f:
    f.write(
        f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2024"\npublish = false\n\n'
        '[lib]\ncrate-type = ["cdylib"]\n\n[dependencies]\nspike-api = { path = "api" }\n\n[workspace]\n'
    )
with open(os.path.join(out, "sunrise-mod.toml"), "w") as f:
    f.write(
        f'id = "{name.replace("-", "_")}"\nname = "Spike"\nversion = "0.1.0"\n'
        'description = "1279 spike."\napi = "0.2"\nside = "both"\ncapabilities = ["registries"]\n'
    )

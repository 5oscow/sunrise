//! Spike for sunrise#1279, question 3: instantiate time of a module with many distinct function imports, each
//! bound by its own `Func::wrap` closure through a callback shaped like `Group::link_cached_with`'s
//! `host(store, module, name) -> Option<Extern>` (crates/sunrise-mod-wasm/src/link.rs, runtime.rs:960).
//! Engine settings are those of `runtime::engine_config` (epoch interruption, guest stack).
use std::time::{Duration, Instant};
use wasmtime::{Config, Engine, Extern, Func, Instance, Module, Store};

fn wat(imports: usize, bodies: bool) -> String {
    let mut s = String::from("(module\n");
    for i in 0..imports {
        s += &format!(
            "(import \"sunrise\" \"call:ServerHost::method_{i}#{:032x}\" (func $i{i} (param i32) (result i32)))\n",
            (i as u128).wrapping_mul(0x9e3779b97f4a7c15f39cc0605cedc835)
        );
    }
    s += "(memory (export \"memory\") 1)\n";
    if bodies {
        for i in 0..imports {
            s += &format!("(func (export \"f{i}\") (param i32) (result i32) local.get 0 call $i{i})\n");
        }
    } else {
        s += "(func (export \"f0\") (param i32) (result i32) local.get 0 call $i0)\n";
    }
    s += ")";
    s
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn main() {
    let mut config = Config::new();
    config.epoch_interruption(true);
    config.max_wasm_stack(1 << 20);
    config.async_stack_size((1 << 20) + (256 << 10));
    let engine = Engine::new(&config).unwrap();
    let dir = std::env::temp_dir().join("spike-1279-cache");
    std::fs::create_dir_all(&dir).unwrap();
    let samples: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(301);
    println!("wasmtime 50.0.1, release, {samples} samples each, median");
    println!("{:<34}{:>14}{:>16}{:>16}", "case", "instantiate", "cached total", "uncached total");
    for (label, n, bodies) in [
        ("1 import", 1, false),
        ("50 imports", 50, false),
        ("300 imports", 300, false),
        ("1 import + 1 body", 1, true),
        ("300 imports + 300 bodies", 300, true),
    ] {
        let bytes = wat::parse_str(wat(n, bodies)).unwrap();
        // The link callback: one Func::wrap per distinct import, each its own closure capturing its index.
        let link = |store: &mut Store<u64>, module: &str, name: &str| -> Option<Extern> {
            if module != "sunrise" {
                return None;
            }
            let id = name.rsplit_once("method_")?.1.split('#').next()?.parse::<u32>().ok()?;
            Some(Extern::Func(Func::wrap(store, move |x: u32| -> u32 { x.wrapping_add(id) })))
        };
        let instantiate = |module: &Module| {
            let mut store = Store::new(&engine, 0u64);
            store.set_epoch_deadline(1 << 30);
            let start = Instant::now();
            let imports: Vec<Extern> = module
                .imports()
                .map(|import| link(&mut store, import.module(), import.name()).unwrap())
                .collect();
            let instance = Instance::new(&mut store, module, &imports).unwrap();
            let took = start.elapsed();
            let f = instance.get_typed_func::<u32, u32>(&mut store, "f0").unwrap();
            assert_eq!(f.call(&mut store, 5).unwrap(), 5);
            took
        };
        let module = Module::new(&engine, &bytes).unwrap();
        let file = dir.join(format!("{n}-{bodies}.cwasm"));
        std::fs::write(&file, module.serialize().unwrap()).unwrap();
        let mut inst = Vec::new();
        let mut cached = Vec::new();
        let mut uncached = Vec::new();
        for _ in 0..samples {
            inst.push(instantiate(&module));
            let start = Instant::now();
            let m = unsafe { Module::deserialize_file(&engine, &file) }.unwrap();
            let took = instantiate(&m);
            cached.push(start.elapsed());
            let _ = took;
        }
        for _ in 0..samples.min(101) {
            let start = Instant::now();
            let m = Module::new(&engine, &bytes).unwrap();
            instantiate(&m);
            uncached.push(start.elapsed());
        }
        let fmt = |v: &Vec<Duration>| format!("{:.1?}", median(v.clone()));
        println!("{label:<34}{:>14}{:>16}{:>16}", fmt(&inst), fmt(&cached), fmt(&uncached));
    }
}

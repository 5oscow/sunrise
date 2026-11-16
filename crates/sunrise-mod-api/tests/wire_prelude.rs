//! `#[derive(Wire)]` in a module whose own items shadow the prelude's names (#1510): the derived
//! code names the standard types, constructors and macros from the crate roots, so it compiles
//! and round-trips there as anywhere.
use mod_api::wire::{Reader, Writer};

mod shadowed {
    #![allow(dead_code, unused_macros)]
    pub struct Vec;
    pub struct Option;
    pub struct Result;
    pub struct Some;
    pub struct Ok;
    pub struct Err;
    macro_rules! vec {
        ($($any:tt)*) => {
            compile_error!("the prelude's vec! is shadowed here")
        };
    }
    macro_rules! stringify {
        ($($any:tt)*) => {
            compile_error!("the prelude's stringify! is shadowed here")
        };
    }
    macro_rules! unreachable {
        ($($any:tt)*) => {
            compile_error!("the prelude's unreachable! is shadowed here")
        };
    }

    #[derive(Debug, PartialEq, mod_api::Wire)]
    pub struct Record {
        pub count: u32,
        pub names: ::std::vec::Vec<::std::string::String>,
    }

    #[derive(Debug, PartialEq, mod_api::Wire)]
    pub enum Shape {
        Empty,
        Sized(u32),
        Named { record: Record },
    }
}

#[test]
fn derive_round_trips_where_the_prelude_names_are_shadowed() {
    use shadowed::{Record, Shape};
    for value in [
        Shape::Empty,
        Shape::Sized(7),
        Shape::Named {
            record: Record {
                count: 2,
                names: vec!["a".into(), "b".into()],
            },
        },
    ] {
        let mut writer = Writer::new();
        writer.put(&value);
        assert_eq!(Reader::decode::<Shape>(writer.bytes()).unwrap(), value);
    }
    let mut unknown = Writer::new();
    unknown.put(&9u32);
    assert!(Reader::decode::<Shape>(unknown.bytes()).is_err());
}

use partial_struct::partial;
use serde::{Deserialize, Serialize};

#[test]
fn test_create_new() {
    fn default_to_true() -> bool {
        true
    }

    #[partial(NewStruct1, with(Default), without(Eq))]
    #[partial(NewStruct2)]
    #[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
    pub(crate) struct OldStruct<T> {
        gen: T,
        #[partial(NewStruct1(skip))]
        f1: u8,
        #[partial(NewStruct2(skip))]
        #[serde(default = "default_to_true")]
        f2: bool
    }

    println!("{:#?}", OldStruct { gen: "old", f1: 5, f2: false });
    println!("{:#?}", NewStruct1 { gen: "s1", f2: true });
    println!("{:#?}", NewStruct2 { gen: "s2", f1: 23 });

    assert_eq!(NewStruct1 { gen: "", f2: false }, NewStruct1::<&str>::default());
    assert_eq!("{\"gen\":\"\",\"f2\":false}", serde_json::to_string(&NewStruct1::<&str>::default()).expect("Failed to serialize NewStruct1"));
    assert_eq!(NewStruct1 { gen: "", f2: true }, serde_json::from_str("{\"gen\":\"\"}").expect("Failed to deserialize NewStruct1"));
}
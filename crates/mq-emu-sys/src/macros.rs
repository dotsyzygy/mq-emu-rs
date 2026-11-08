/// Declares the imports from one DLL and, alongside them, a list of every
/// declared symbol name. The list lets tests check the declarations against
/// the DLL's real export table, so the two can't drift apart unnoticed.
macro_rules! imports {
    (
        dll: $dll:literal,
        names: $names:ident,
        functions {
            $( $(#[$fattr:meta])* pub fn $fname:ident ( $($fargs:tt)* ) $(-> $fret:ty)? ; )*
        }
        statics {
            $( $(#[$sattr:meta])* pub static mut $sname:ident : $sty:ty ; )*
        }
        $(
        // C++ member functions, by their mangled export names: `__thiscall`,
        // `this` first. `$sigs` lists each one's name, parameters and return
        // type, so tests can check the Rust types against the mangled name.
        methods($sigs:ident) {
            $( $(#[$mattr:meta])* pub fn $mname:ident ( $($margs:tt)* ) $(-> $mret:ty)? = $mlink:literal ; )*
        }
        )?
        $(
        // C++ functions that aren't members, and static members, by their
        // mangled export names: `__cdecl`. `$csigs` lists them as `$sigs` does.
        cpp_functions($csigs:ident) {
            $( $(#[$cattr:meta])* pub fn $cname:ident ( $($cargs:tt)* ) $(-> $cret:ty)? = $clink:literal ; )*
        }
        )?
    ) => {
        #[link(name = $dll)]
        unsafe extern "C" {
            $( $(#[$fattr])* pub fn $fname( $($fargs)* ) $(-> $fret)?; )*
            $( $(#[$sattr])* pub static mut $sname: $sty; )*
        }

        $(
        #[link(name = $dll)]
        unsafe extern "C" {
            $( $(#[$cattr])* #[link_name = $clink] pub fn $cname( $($cargs)* ) $(-> $cret)?; )*
        }
        )?

        $(
        #[link(name = $dll)]
        unsafe extern "thiscall" {
            $( $(#[$mattr])* #[link_name = $mlink] pub fn $mname( $($margs)* ) $(-> $mret)?; )*
        }
        )?

        /// Every symbol name declared in this module, as exported by the DLL.
        #[doc(hidden)]
        pub const $names: &[&str] = &[ $(stringify!($fname),)* $(stringify!($sname),)* $($($mlink,)*)? $($($clink,)*)? ];

        $(
        /// Every method declared in this module: its mangled name, then its
        /// Rust parameters and return type (empty for none) as written.
        #[doc(hidden)]
        pub const $sigs: &[(&str, &str, &str)] = &[ $( ($mlink, stringify!($($margs)*), concat!("" $(, stringify!($mret))?)), )* ];
        )?

        $(
        /// Every C++ function declared in this module, as the methods' list.
        #[doc(hidden)]
        pub const $csigs: &[(&str, &str, &str)] = &[ $( ($clink, stringify!($($cargs)*), concat!("" $(, stringify!($cret))?)), )* ];
        )?
    };
}

/// Declares shim functions and `Api`: the same functions as a table of
/// pointers, with `API` holding the real ones.
///
/// Calling through a table keeps a DLL the shim imports (imgui.dll,
/// MQ2Main.dll) out of programs that don't use it. Code calling the functions
/// directly lands in some object file, and every program linking that object
/// file imports the DLL, used or not; it then won't start without it. The
/// pointers in `API` are only materialized where it's used, so code that takes
/// it in a plugin (say, through a generic function only the plugin
/// instantiates) adds the import to the plugin alone.
///
/// Functions under `rust { }` are defined in Rust in the same module (as
/// `pub unsafe extern "C" fn`), replacing a shim function under the same name
/// and type: callers and fakes of `Api` don't change.
macro_rules! function_table {
    (
        $( $(#[$meta:meta])* pub fn $name:ident($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)?; )*
        $( rust {
            $( $(#[$rmeta:meta])* pub fn $rname:ident($($rarg:ident: $rty:ty),* $(,)?) $(-> $rret:ty)?; )*
        } )?
    ) => {
        unsafe extern "C" {
            $( $(#[$meta])* pub fn $name($($arg: $ty),*) $(-> $ret)?; )*
        }

        /// Every function of this module as a pointer; see [`API`].
        #[derive(Clone, Copy)]
        pub struct Api {
            $( pub $name: unsafe extern "C" fn($($ty),*) $(-> $ret)?, )*
            $($( $(#[$rmeta])* pub $rname: unsafe extern "C" fn($($rty),*) $(-> $rret)?, )*)?
        }

        /// The real functions. Taking this in code every program links (rather
        /// than only plugins) makes those programs import the shim's DLLs.
        pub const API: Api = Api { $( $name, )* $($( $rname, )*)? };
    };
}

#[cfg(test)]
mod tests {
    #![allow(dead_code)]

    use core::ffi::c_char;

    // The declarations are never referenced, so no MQ2Main.dll import is generated;
    // the .lib only has to exist for the linker.
    imports! {
        dll: "MQ2Main",
        names: TEST_IMPORTS,
        functions {
            /// Doc comments and variadics must pass through.
            pub fn VariadicFn(format: *const c_char, ...);
            pub fn ReturnsValue(a: i32) -> bool;
        }
        statics {
            pub static mut SomeGlobal: [c_char; 4];
        }
        methods(TEST_METHODS) {
            pub fn Thing_Get(this: *const c_char, index: i32) -> bool = "?Get@Thing@@QBE_NH@Z";
            pub fn Thing_drop(this: *mut c_char) = "??1Thing@@QAE@XZ";
        }
        cpp_functions(TEST_CPP_FUNCTIONS) {
            pub fn Thing_Count() -> i32 = "?Count@Thing@@SAHXZ";
        }
    }

    imports! {
        dll: "MQ2Main",
        names: EMPTY_IMPORTS,
        functions {}
        statics {}
    }

    #[test]
    fn names_list_every_function_then_static() {
        assert_eq!(TEST_IMPORTS, ["VariadicFn", "ReturnsValue", "SomeGlobal", "?Get@Thing@@QBE_NH@Z", "??1Thing@@QAE@XZ", "?Count@Thing@@SAHXZ"]);
    }

    #[test]
    fn signatures_list_each_method_with_its_rust_types() {
        // Compares ignoring whitespace, which `stringify!` may add.
        let same = |a: &str, b: &str| a.chars().filter(|c| !c.is_whitespace()).eq(b.chars().filter(|c| !c.is_whitespace()));
        let expected = [("?Get@Thing@@QBE_NH@Z", "this: *const c_char, index: i32", "bool"), ("??1Thing@@QAE@XZ", "this: *mut c_char", "")];
        assert_eq!(TEST_METHODS.len(), expected.len());
        for (&(link, args, ret), (elink, eargs, eret)) in TEST_METHODS.iter().zip(expected) {
            assert!(link == elink && same(args, eargs) && same(ret, eret), "{link}: ({args}) -> {ret}");
        }
        assert!(matches!(TEST_CPP_FUNCTIONS, [("?Count@Thing@@SAHXZ", "", ret)] if same(ret, "i32")), "{TEST_CPP_FUNCTIONS:?}");
    }

    #[test]
    fn empty_imports_have_no_names() {
        assert!(EMPTY_IMPORTS.is_empty());
    }
}

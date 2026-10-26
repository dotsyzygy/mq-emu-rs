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
    ) => {
        #[link(name = $dll)]
        unsafe extern "C" {
            $( $(#[$fattr])* pub fn $fname( $($fargs)* ) $(-> $fret)?; )*
            $( $(#[$sattr])* pub static mut $sname: $sty; )*
        }

        /// Every symbol name declared in this module, as exported by the DLL.
        #[doc(hidden)]
        pub const $names: &[&str] = &[ $(stringify!($fname),)* $(stringify!($sname),)* ];
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
macro_rules! function_table {
    ($( $(#[$meta:meta])* pub fn $name:ident($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)?; )*) => {
        unsafe extern "C" {
            $( $(#[$meta])* pub fn $name($($arg: $ty),*) $(-> $ret)?; )*
        }

        /// Every function of this module as a pointer; see [`API`].
        #[derive(Clone, Copy)]
        pub struct Api {
            $( pub $name: unsafe extern "C" fn($($ty),*) $(-> $ret)?, )*
        }

        /// The real functions. Taking this in code every program links (rather
        /// than only plugins) makes those programs import the shim's DLLs.
        pub const API: Api = Api { $( $name, )* };
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
    }

    imports! {
        dll: "MQ2Main",
        names: EMPTY_IMPORTS,
        functions {}
        statics {}
    }

    #[test]
    fn names_list_every_function_then_static() {
        assert_eq!(TEST_IMPORTS, ["VariadicFn", "ReturnsValue", "SomeGlobal"]);
    }

    #[test]
    fn empty_imports_have_no_names() {
        assert!(EMPTY_IMPORTS.is_empty());
    }
}

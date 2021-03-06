use analysis::ref_mode::RefMode;
use analysis::rust_type::*;
use analysis::imports::Imports;
use analysis::namespaces;
use analysis::override_string_type::override_string_type_return;
use config;
use env::Env;
use library::{self, Nullable, TypeId};

#[derive(Clone, Debug, Default)]
pub struct Info {
    pub parameter: Option<library::Parameter>,
    pub base_tid: Option<library::TypeId>, // Some only if need downcast
    pub commented: bool,
    pub bool_return_is_error: Option<String>,
}

pub fn analyze(
    env: &Env,
    func: &library::Function,
    type_tid: library::TypeId,
    configured_functions: &[&config::functions::Function],
    used_types: &mut Vec<String>,
    imports: &mut Imports,
) -> Info {
    let typ = override_string_type_return(env, func.ret.typ, configured_functions);
    let mut parameter = if typ == Default::default() {
        None
    } else {
        if let Ok(s) = used_rust_type(env, typ, false) {
            used_types.push(s);
        }
        // Since GIRs are bad at specifying return value nullability, assume
        // any returned pointer is nullable unless overridden by the config.
        let mut nullable = func.ret.nullable;
        if !*nullable && can_be_nullable_return(env, typ) {
            *nullable = true;
        }
        let nullable_override = configured_functions
            .iter()
            .filter_map(|f| f.ret.nullable)
            .next();
        if let Some(val) = nullable_override {
            nullable = val;
        }
        Some(library::Parameter {
            typ,
            nullable,
            ..func.ret.clone()
        })
    };

    let commented = if typ == Default::default() {
        false
    } else {
        parameter_rust_type(
            env,
            typ,
            func.ret.direction,
            Nullable(false),
            RefMode::None,
        ).is_err()
    };

    let bool_return_is_error = configured_functions
        .iter()
        .filter_map(|f| f.ret.bool_return_is_error.as_ref())
        .next();
    let bool_return_error_message =
        bool_return_is_error.and_then(|m| if typ != TypeId::tid_bool() {
            error!(
                "Ignoring bool_return_is_error configuration for non-bool returning function {}",
                func.name
            );
            None
        } else {
            let ns = if env.namespaces.glib_ns_id == namespaces::MAIN {
                "error"
            } else {
                "glib"
            };
            imports.add(ns, None);

            Some(m.clone())
        });

    let mut base_tid = None;

    if func.kind == library::FunctionKind::Constructor {
        if let Some(par) = parameter {
            let nullable_override = configured_functions
                .iter()
                .filter_map(|f| f.ret.nullable)
                .next();
            if par.typ != type_tid {
                base_tid = Some(par.typ);
            }
            parameter = Some(library::Parameter {
                typ: type_tid,
                nullable: nullable_override.unwrap_or(Nullable(false)),
                ..par
            });
        }
    }

    Info {
        parameter,
        base_tid,
        commented,
        bool_return_is_error: bool_return_error_message,
    }
}

fn can_be_nullable_return(env: &Env, type_id: library::TypeId) -> bool {
    use library::Type::*;
    use library::Fundamental::*;
    match *env.library.type_(type_id) {
        Fundamental(fund) => match fund {
            Pointer => true,
            Utf8 => true,
            Filename => true,
            OsString => true,
            _ => false,
        },
        Alias(ref alias) => can_be_nullable_return(env, alias.typ),
        Enumeration(_) => false,
        Bitfield(_) => false,
        Record(_) => true,
        Union(_) => true,
        Function(_) => true,
        Interface(_) => true,
        Class(_) => true,
        _ => true,
    }
}

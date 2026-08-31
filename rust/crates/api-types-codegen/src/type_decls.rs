use std::any::TypeId;
use std::collections::{BTreeMap, HashSet};
use std::panic::{catch_unwind, set_hook, take_hook, AssertUnwindSafe, PanicHookInfo};
use std::sync::Mutex;

use ts_rs::{Config, TypeVisitor, TS};

static PANIC_HOOK_LOCK: Mutex<()> = Mutex::new(());

type RootRegistrar = fn(&Config, &mut BTreeMap<String, String>, &mut HashSet<TypeId>);
type ElectronExtraRoot = (&'static str, &'static str, RootRegistrar);

const ELECTRON_EXTRA_ROOTS: &[ElectronExtraRoot] = &[
    (
        "BoolTouchedResponse",
        "Preserves the existing standalone mutation-result export; no route currently references it.",
        register_decl::<api_types::BoolTouchedResponse>,
    ),
    (
        "EmptyResponse",
        "Preserves the canonical zero-field response export; no route currently references it.",
        register_decl::<api_types::EmptyResponse>,
    ),
    (
        "ErrorResponse",
        "HTTP callers parse this shared non-success body outside per-route response metadata.",
        register_decl::<api_types::ErrorResponse>,
    ),
];

include!(concat!(env!("OUT_DIR"), "/route_roots.rs"));

pub fn known_type_decls() -> BTreeMap<String, String> {
    let cfg = Config::new().with_large_int("number");
    let mut decls = BTreeMap::new();
    let mut seen = HashSet::new();

    register_route_roots(&cfg, &mut decls, &mut seen);
    for &(_name, _reason, register) in ELECTRON_EXTRA_ROOTS {
        register(&cfg, &mut decls, &mut seen);
    }

    decls
}

fn register_decl<T: TS + 'static>(
    cfg: &Config,
    decls: &mut BTreeMap<String, String>,
    seen: &mut HashSet<TypeId>,
) {
    register_decl_dyn::<T>(cfg, decls, seen);
}

fn register_decl_dyn<T: TS + 'static + ?Sized>(
    cfg: &Config,
    decls: &mut BTreeMap<String, String>,
    seen: &mut HashSet<TypeId>,
) {
    if !seen.insert(TypeId::of::<T>()) {
        return;
    }

    if let Some(name) = catch_quiet(|| T::ident(cfg)).filter(|name| should_emit_decl(name)) {
        if let Some(decl) = catch_quiet(|| T::decl_concrete(cfg)) {
            decls
                .entry(name)
                .or_insert_with(|| format!("export {}\n", decl));
        }
    }

    struct Visit<'a> {
        cfg: &'a Config,
        decls: &'a mut BTreeMap<String, String>,
        seen: &'a mut HashSet<TypeId>,
    }

    impl TypeVisitor for Visit<'_> {
        fn visit<U: TS + 'static + ?Sized>(&mut self) {
            register_decl_dyn::<U>(self.cfg, self.decls, self.seen);
        }
    }

    let mut visit = Visit { cfg, decls, seen };
    T::visit_dependencies(&mut visit);
    T::visit_generics(&mut visit);
}

fn should_emit_decl(name: &str) -> bool {
    let Some(first) = name.chars().next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn catch_quiet<T>(f: impl FnOnce() -> T) -> Option<T> {
    let _hook_guard = PANIC_HOOK_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let hook = take_hook();
    set_hook(Box::new(|_: &PanicHookInfo<'_>| {}));
    let result = catch_unwind(AssertUnwindSafe(f)).ok();
    set_hook(hook);
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn generated_route_roots_match_runtime_registry() {
        mando_gateway::contract_inventory_link_anchor();
        let runtime_roots: BTreeSet<_> = api_types::route_registrations()
            .into_iter()
            .flat_map(|route| {
                [
                    route.body_ty,
                    route.query_ty,
                    route.params_ty,
                    route.res_ty,
                    route.event_ty,
                ]
                .into_iter()
                .flatten()
            })
            .collect();
        let generated_roots: BTreeSet<_> = GENERATED_ROUTE_ROOTS.iter().copied().collect();

        assert_eq!(generated_roots, runtime_roots);
    }

    #[test]
    fn electron_extra_roots_are_named_and_explained() {
        let decls = known_type_decls();
        for &(name, reason, _) in ELECTRON_EXTRA_ROOTS {
            assert!(
                decls.contains_key(name),
                "extra root {name} was not emitted"
            );
            assert!(
                reason.split_whitespace().count() >= 5,
                "extra root {name} needs a specific audit reason"
            );
        }
    }
}

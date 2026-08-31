use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    mando_gateway::contract_inventory_link_anchor();

    let route_roots: BTreeSet<_> = api_types::route_registrations()
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

    let mut generated = String::from(
        "// Generated from the typed route registry by build.rs.\n\
         #[cfg(test)]\n\
         const GENERATED_ROUTE_ROOTS: &[&str] = &[\n",
    );
    for rust_type in &route_roots {
        generated.push_str(&format!("    {rust_type:?},\n"));
    }
    generated.push_str(
        "];\n\n\
         fn register_route_roots(\n\
             cfg: &Config,\n\
             decls: &mut BTreeMap<String, String>,\n\
             seen: &mut HashSet<TypeId>,\n\
         ) {\n",
    );
    for rust_type in route_roots {
        generated.push_str(&format!(
            "    register_decl::<{rust_type}>(cfg, decls, seen);\n"
        ));
    }
    generated.push_str("}\n");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").context("OUT_DIR is not set")?);
    fs::write(out_dir.join("route_roots.rs"), generated)
        .context("failed to write generated route root registrations")?;
    Ok(())
}

//! Prints the generated schema module of the DRY's JavaScript tools
//! (`bsc/tools/dbrec_schemas.generated.cjs`).

fn main() {
    print!("{}", darkbot_bsc_record::js::module());
}

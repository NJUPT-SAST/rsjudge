use tonic_build::compile_protos;

fn main() -> anyhow::Result<()> {
    compile_protos("proto/rsjudge/v1/rsjudge.proto")?;
    Ok(())
}

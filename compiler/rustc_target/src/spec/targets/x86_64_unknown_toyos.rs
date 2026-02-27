use crate::spec::{Arch, StackProbeType, Target, TargetMetadata, base};

pub(crate) fn target() -> Target {
    let mut opts = base::toyos::opts();
    opts.cpu = "x86-64".into();
    opts.max_atomic_width = Some(64);
    opts.stack_probes = StackProbeType::Inline;

    Target {
        llvm_target: "x86_64-unknown-none-elf".into(),
        metadata: TargetMetadata {
            description: Some("x86_64 ToyOS".into()),
            tier: Some(3),
            host_tools: Some(true),
            std: Some(true),
        },
        pointer_width: 64,
        data_layout:
            "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
                .into(),
        arch: Arch::X86_64,
        options: opts,
    }
}

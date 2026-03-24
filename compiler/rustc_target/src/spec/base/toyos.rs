use crate::spec::{
    Cc, FramePointer, LinkerFlavor, Lld, Os, RelocModel, StackProbeType, TargetOptions,
};

pub(crate) fn opts() -> TargetOptions {
    TargetOptions {
        os: Os::ToyOs,
        linker: Some("toyos-ld".into()),
        linker_flavor: LinkerFlavor::Gnu(Cc::No, Lld::No),
        stack_probes: StackProbeType::Inline,
        relocation_model: RelocModel::Pic,
        position_independent_executables: true,
        has_thread_local: true,
        main_needs_argc_argv: false,
        default_uwtable: true,
        frame_pointer: FramePointer::Always,
        dynamic_linking: true,
        dll_prefix: "lib".into(),
        dll_suffix: ".so".into(),
        has_rpath: false,
        ..Default::default()
    }
}

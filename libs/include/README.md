# libs/include

The C headers that Haskell sources (through CPP), hsc2hs and packages' C code include.

- GHC 9.12.2's runtime headers, as its `rts` package installs them: `Rts.h`, `RtsAPI.h`,
  `HsFFI.h`, `Stg.h`, `stg/`, `rts/`, `Cmm.h`, `Jumps.h`, `DerivedConstants.h`,
  `ghcautoconf.h`, `ghcversion.h`, `ghcconfig.h` (BSD-3-Clause, the GHC Team). Packages such as
  ghc-lib-parser (GHC.Exts.Heap) compile against their structure layouts. hsc's own runtime is
  not GHC's: it implements only what `libs/rts/hsc-rts-shim.c` and the Rust runtime provide.
- libffi's `ffi.h` and `ffitarget.h`, as GHC installs them (MIT).
- hsc's own: `MachDeps.h` and `ghcplatform.h` (GHC's, with more of the spellings sources test),
  `HsBase.h`, `HsBaseConfig.h`, `CTypes.h`, `EventConfig.h`, `ieee-flpt.h` (base's headers and
  configure results for x86-64 Linux).

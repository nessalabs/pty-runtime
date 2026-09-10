# Ordinary guardian spawn failure

The frozen candidate6 Linux gate failed before raw detach assertions. Two hundred repetitions under strace passed; the subsequent untraced run in a separate checkout with error-only parent diagnostics failed on repetition37 with initial guardian exec ExecutableFileBusy/errno26. The driver stopped at that first failure. These observations establish the stage and errno, not the interleaving by themselves. The independent deterministic real-constructor RED test in ../image-fork/red separately demonstrates inherited-writer ownership.

The diagnostic-only source changes and build source inventory are in ../spawn-diagnostic6. Frozen candidate6 and its running macOS measurements were not edited. The full gate remains failed until a reviewed fix and new validation; the original failure is not erased by the passing CI run.

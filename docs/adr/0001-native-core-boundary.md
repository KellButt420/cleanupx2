# A versioned C ABI with polled operations

The frontend uses WinUI 3 and owns presentation and user confirmation; Rust owns inventory and official uninstall operations. ABI version 1 uses length-delimited UTF-8 JSON batches, opaque operation handles, and explicit Rust buffer release, accepting serialization overhead to keep ownership auditable and avoid unstable Rust layouts or callbacks into managed code. Polling runs off the UI thread, workers catch panics, and cancellation stops discovery or waiting without terminating an installer that may be changing system state.

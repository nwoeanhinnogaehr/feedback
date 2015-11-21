# feedback
This project allows you to send sound in an LADSPA effect chain to arbitrary other locations in the chain, inducing a small amount of necessary network delay in the process. Eventually this delay amount will be configurable, but for now it's tied to the actual latency of your TCP stack.

This has only been tested on Linux, but it probably works on Mac as well. Windows is not supported at the moment because mio doesn't support it. There's one binary, libfeedback.so, which either should be in your `LADSPA_PATH` or wherever your OS puts LADSPA plugins. The binary contains two plugins, the "Feedback Transmitter" and "Feedback Receiver". Just add both of them somewhere in your DAW (tested in Renoise and LMMS), make sure they are on the same channel, and you are good to go!

To build, you need the latest Rust nightly, then just do `cargo build --release` and look at `target/release/libfeedback.so`.

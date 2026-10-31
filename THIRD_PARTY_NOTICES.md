# Third-party notices

Notices for code shipped in OpenDeck builds that requires one beyond `Cargo.lock` and `cargo deny`. Rust and npm dependency licences are listed by those tools.

## VST 3 interface headers (builds with the `vst3` feature only)

The `vst3` crate (MIT OR Apache-2.0) ships Rust bindings generated from Steinberg's VST 3 interface headers, fork `coupler-rs/vst3_pluginterfaces` (source: `LICENSE.txt` of that repository; upstream SDK `steinbergmedia/vst3sdk`, also MIT). OpenDeck does not use the VST logo (ADR 0008).

```
MIT License

Copyright (c) 2025, Steinberg Media Technologies GmbH

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.


```

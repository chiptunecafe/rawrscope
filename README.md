# rawrscope

rawrscope is a fast, user-friendly, and cross-platform tool for creating
oscilloscope visualizations of audio, typically chiptune.

*TODO preview screenshot*

## Important Note on macOS Support

**Currently, bugs specific to macOS likely will not be resolved!** As of macOS
10.14, OpenGL, the graphics API that rawrscope currently uses, has been
deprecated in favor of Apple's Metal API. Although Pathfinder, the 2D rendering
engine rawrscope uses, has a Metal backend, I would rather not have to
explicitly support it. Instead, I am waiting on Pathfinder to implement a
`[gfx-hal](https://github.com/gfx-rs/gfx)` backend that will allow rawrscope to
target all of the major graphics APIs.

More on Pathfinder `gfx-hal` support:
* [PR for new Pathfinder GPU API](https://github.com/servo/pathfinder/pull/213)
* [bzm3r's WIP fork](https://github.com/bzm3r/pathfinder/tree/pf3-gfx-hal/)

## Features

* Intuitive interface
* Realtime editor and preview
* Fast and high quality 2D rendering using the
  [Pathfinder](https://github.com/servo/pathfinder) rasterizer
* Many centering algorithms
  * Peak Speed
  * Fundamental Phase
  * Crosscorrelation
  * External Trigger
* High-quality trigger generator for external trigger mode
* Audio manipulation tools (trim, fade in/out, gain)
* Node-based audio routing interface
  * Automatic master audio generation
  * Stereo upmixing/downmixing
* Visual templates and presets for a quicker workflow
* Built-in video export
* Arbitrary post-processing shaders
* Command line interface
* Wriiten in [Rust](https://www.rust-lang.org) :)

## Installation

*TODO*

## Tutorial

*TODO*

## Future Roadmap

* Full timeline editor for audio sources
* Integrated rendering of chiptune files
* Scripting support
# Project Resonant — Architecture Overview

**Status:** Initial architectural direction  
**Last updated:** 2026-11-16

## 1. Product Goal

Project Resonant is an open-source, professional-grade digital audio
workstation built with Rust.

The intended creative workflow combines:

- Session-based clip launching
- Linear arrangement and recording
- Pattern sequencing
- MIDI editing and piano roll
- Mixing, instruments, effects, and automation

These workflows should operate on one coherent project model.

## 2. Architectural Layers

The architecture is organized around clear responsibilities.

### Application

Owns application lifecycle, windows, workspace layout, user commands,
selection, and coordination of application services.

### User Interface and Rendering

Owns custom UI components, layout, input, themes, text, and GPU rendering.
It communicates with application services rather than directly controlling
the audio callback.

### Project and Musical Model

Owns tracks, clips, scenes, patterns, MIDI events, automation, routing,
tempo maps, and persistent project state.

### Audio Engine

Owns real-time scheduling, audio processing, graph execution, mixing,
DSP, and rendering.

### Plugin Platform

Owns plugin discovery, loading, lifecycle, parameters, state, host
integration, and the public plugin development interface.

### Platform Integration

Owns operating-system integration, audio and MIDI devices, filesystem
access, and native APIs.

## 3. Real-Time Audio Safety

The audio processing thread is a real-time execution environment.

The design must avoid:

- Heap allocation during audio processing
- Blocking mutexes and unbounded waits
- Filesystem and network operations
- UI calls or synchronous UI communication
- Unbounded work or unpredictable destruction

Resources and graph changes should be prepared away from the audio
thread and transferred through bounded, real-time-safe mechanisms.

These requirements must be validated through prototypes and tests.

## 4. Time Domains

Musical time and audio time are separate concepts.

Musical time represents beats, bars, ticks, tempo, and musical position.

Audio time represents sample frames, sample rate, and processing position.

Conversions must account for tempo changes and support accurate scheduling.

## 5. Safety and Native Integration

Safe Rust is the default.

Unsafe code may be necessary at carefully defined boundaries for native
APIs, foreign-function interfaces, and specialized low-level operations.

Such code must be isolated, justified, documented, and reviewed.
Safe public abstractions should prevent callers from needing unsafe code.

## 6. Project Persistence

Project files need explicit schema versions, validation, migration
support, and reliable save/recovery behavior.

The eventual native project format is provisionally `.rproj`.

Interchange with established audio and MIDI formats will be evaluated.

## 7. Plugin Strategy

The plugin platform must distinguish the stable public plugin ABI from
internal Rust implementation details.

CLAP is an initial external plugin format candidate. VST3, Audio Units,
and LV2 support will be evaluated by platform, licensing, and feasibility.

The native plugin SDK will receive a separately versioned interface.

## 8. Incremental Development

The architecture is a target, not a mandate to create every subsystem
immediately.

New crates should be introduced when a responsibility becomes concrete.

Every milestone should include implementation, verification,
documentation, and a demonstrable result.

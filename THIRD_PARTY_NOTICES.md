# Third-Party Notices & Provenance

ChatGPTX is distributed under the [GNU Affero General Public License v3.0](LICENSE) (`AGPL-3.0-only`). This document records the upstream projects, adapted components, reference implementations, and third-party license notices associated with this repository.

---

## 1. Upstream Base Project — CodexPlusPlus

ChatGPTX is a derivative work based on **CodexPlusPlus**:

- **Repository**: <https://github.com/BigPizzaV3/CodexPlusPlus>
- **License**: GNU Affero General Public License v3.0 (`AGPL-3.0-only`)
- **Copyright**: Copyright (C) 2026 BigPizzaV3 and contributors
- **Derivative Modifications**: Copyright (C) 2026 aurorax-neo (<https://github.com/aurorax-neo/chatgptx>)

Summary of major modifications in ChatGPTX (2026-10):
- Rebranded application package, state directory (`~/.chatgptx`), and bundle identifiers to ChatGPTX.
- Removed sponsor advertisements, WeChat bridge, Grok/Claude platform modules, and DreamSkin market modules.
- Added multi-account official OAuth PKCE login, automatic token refresh, real-time quota inspection, macOS Keychain synchronization, per-account third-party API provider binding, official account multi-instance launch, main-instance stop control, and unified global network proxy settings.

In accordance with AGPL-3.0 Section 5 and Section 13, the complete corresponding source code of ChatGPTX is publicly available at <https://github.com/aurorax-neo/chatgptx>.

---

## 2. Design & Workflow Reference — cockpit-tools

The multi-account official OAuth management, token refresh lifecycle, quota display, and global proxy workflow in ChatGPTX reference the design of **cockpit-tools**:

- **Repository**: <https://github.com/jlcodes99/cockpit-tools>
- **Author**: jlcodes (`jlcodes99`)
- **License**: Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International (`CC-BY-NC-SA-4.0`, <https://creativecommons.org/licenses/by-nc-sa/4.0/>)
- **Notice**: ChatGPTX is provided free of charge for personal learning, research, and non-commercial use. Respect the upstream author's attribution and non-commercial terms when referencing or reusing related designs.

---

## 3. Built-in Usage Widget — DeepSeek-Balance-Whale-Widget & Codex-Whale-Public

The optional usage widget and desktop pet engine in `assets/inject/upstream/whale-widget/` are adapted from:

- **Primary Upstream**: <https://github.com/MeteorNOX/DeepSeek-Balance-Whale-Widget>
- **Codex Desktop Adaptation Reference**: <https://github.com/Yang-huai406/Codex-Whale-Public>
- **Code License**: MIT License (see [`assets/inject/upstream/whale-widget/LICENSE`](assets/inject/upstream/whale-widget/LICENSE))
- **Media & Artwork Provenance**: Character images, animations, and audio files under `assets/inject/upstream/whale-widget/assets/` are governed by [`assets/inject/upstream/whale-widget/PROVENANCE.md`](assets/inject/upstream/whale-widget/PROVENANCE.md) (provided as-is for running the widget and not claimed as original works of this repository).

```text
MIT License

Copyright (c) 2026 MeteorNOX

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

---

## 4. Local Speech Recognition Runtime — Sherpa-ONNX

- **Repository**: <https://github.com/k2-fsa/sherpa-onnx>
- **License**: Apache License 2.0 (`Apache-2.0`)
- **Notice**: Local speech recognition models (such as SenseVoiceSmall) are downloaded on demand by the user and remain subject to their respective model licenses.

---

## 5. Trademarks & Disclaimer

- OpenAI, ChatGPT, and Codex are trademarks or registered trademarks of OpenAI, L.L.C.
- ChatGPTX is an independent open-source project and is not affiliated with, endorsed by, or sponsored by OpenAI.
- The open-source licenses of this repository cover ChatGPTX code only and do not grant any rights to third-party trademarks, official desktop application binaries, or proprietary service terms.

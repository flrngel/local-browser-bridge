# 조사 결과와 설계 근거

조사일: 2026-08-17

## 제품 기능 조사

OpenAI 공식 문서상 Codex Chrome extension은 기존 Chrome profile, 로그인 세션, 열린 탭, 확장 상태를 사용하며 페이지 읽기/조작, 열린 탭과 선택 텍스트를 chat context로 가져오기, YouTube timestamped transcript 사용, 사이트별 allow/block, 민감 동작 확인을 제공합니다. built-in browser는 별도 profile을 쓰고 localhost preview에 적합합니다.

- [OpenAI: Chrome extension](https://learn.chatgpt.com/docs/chrome-extension)
- [OpenAI: Browser](https://learn.chatgpt.com/docs/browser)
- [OpenAI: Computer Use](https://learn.chatgpt.com/docs/computer-use)

Microsoft 공식 문서상 M365 Copilot Cowork local browser는 사용자 기기의 Edge hidden tab에서 실행되며 기존 SSO/cookie/session을 사용합니다. 현재 Cowork web + Edge에서만 지원되고, 민감 변경은 approval card를 요구합니다. 따라서 이 프로젝트의 localhost UI는 **Cowork local browser**와 결합할 수 있지만 Microsoft-managed hosted browser에는 연결할 수 없습니다.

- [Microsoft: Use the local browser with Copilot Cowork](https://learn.microsoft.com/en-us/microsoft-365/copilot/cowork/cowork-local-browser)
- [Microsoft: Configure where computer use runs](https://learn.microsoft.com/en-us/microsoft-copilot-studio/configure-where-computer-use-runs)

## 확장 플랫폼 근거

Manifest V3 extension service worker는 Chrome 116부터 30초보다 짧은 WebSocket keepalive traffic으로 연결 수명을 유지할 수 있습니다. 그래서 native messaging host나 open CDP port 대신 확장이 `127.0.0.1`로 outbound WebSocket을 여는 구조를 선택했습니다.

- [Chrome: Use WebSockets in service workers](https://developer.chrome.com/docs/extensions/how-to/web-platform/websockets)
- [Chrome: Extension service worker lifecycle](https://developer.chrome.com/docs/extensions/develop/concepts/service-workers/lifecycle)

Chrome 보안 가이드는 최소 권한, 명시적 CSP, content script의 불신, 메시지/입력 검증, 민감한 Chrome API 작업을 service worker에 두는 것을 권고합니다. 구현은 DOM snapshot/ref 해석만 isolated content script에 두고, mode policy·WebSocket·tab/debugger 권한을 service worker에서 처리합니다. 사용자가 요청한 Full Access는 이 권고보다 기능성을 우선하는 명시적 고위험 모드이며 Safe mode로 되돌릴 수 있습니다.

- [Chrome: Stay secure](https://developer.chrome.com/docs/extensions/develop/security-privacy/stay-secure)
- [Chrome: Protect user privacy](https://developer.chrome.com/docs/extensions/develop/security-privacy/user-privacy)

`captureVisibleTab`은 초당 약 2회 제한이 있으므로 capture를 window별 550ms 간격으로 throttle합니다.

- [Chrome: chrome.tabs / captureVisibleTab](https://developer.chrome.com/docs/extensions/reference/api/tabs#method-captureVisibleTab)

## 연구 문헌

WebArena는 realistic multi-step web task가 단순 synthetic task보다 훨씬 어렵고 초기 GPT-4 baseline의 end-to-end 성공률이 14.41%였음을 보였습니다. BrowserGym은 browser agent의 observation/action space를 명시적으로 분리하고 고수준 action primitive가 임의 코드보다 제어 가능하다는 방향을 정리합니다. Safe mode는 작은 action vocabulary를 유지하고, Full Access는 사이트 호환성을 위해 좌표·자유 키·임의 JS escape hatch를 추가합니다.

- [WebArena: A Realistic Web Environment for Building Autonomous Agents](https://arxiv.org/abs/2307.13854)
- [The BrowserGym Ecosystem for Web Agent Research](https://arxiv.org/abs/2412.05467)
- [OSWorld: Benchmarking Multimodal Agents for Open-Ended Tasks in Real Computer Environments](https://arxiv.org/abs/2404.07972)

스크린샷만 또는 DOM만 사용하는 대신 두 observation을 함께 제공합니다. 화면 픽셀과 구조화 표현은 서로 다른 실패 모드를 가지며, 에이전트는 실제 성공 상태를 재관찰해야 합니다.

## 공개 구현과 커뮤니티 신호

공개 browser bridge 구현과 커뮤니티 사례에서 반복되는 dominant pattern은 `local server + MV3 extension + outbound WebSocket + loopback bind + token auth`였습니다. 실제 로그인 profile을 보존하면서 open debug port와 native host 설치를 피하는 장점이 있습니다. 이 프로젝트는 그 전송 형태를 채택하되 MCP client 대신 browser-only agent가 조작할 localhost UI를 추가했습니다.

- [vitalysim/browser-bridge](https://github.com/vitalysim/browser-bridge) — MIT, local server + MV3 + outbound WebSocket
- [koltyakov/browser-bridge](https://github.com/koltyakov/browser-bridge) — 명시적으로 enable한 browser scope와 structured browser access
- [Community discussion: Browser Bridge](https://www.reddit.com/r/ClaudeAI/comments/1v5fz09/browser_bridge_an_mcp_server_that_drives_your/) — real-session bridge 수요, per-site allow와 risky-action confirmation 논의

이 저장소의 코드는 위 프로젝트에서 복사하지 않았으며, 공개 아키텍처와 공식 API 문서를 근거로 독립 작성했습니다.

## 기능 범위 결정

| 조사된 기능 | 이 구현 | 결정 |
|---|---:|---|
| 로그인된 실제 browser profile | 예 | 핵심 목적 |
| 탭 목록/전환/이동 | 예 | browser-only agent의 기본 action space |
| 스크린샷 + DOM/텍스트 | 예 | 상호 보완 observation |
| 선택 텍스트 | 예 | 좁은 context 전달 |
| 사이트 allow/block | Full Access: 전체 / Safe: allowlist | 기능 우선 기본값과 되돌릴 수 있는 안전 모드 |
| 민감 동작 확인 | Full Access: 즉시 / Safe: popup approval | 사용자가 선택하는 policy |
| YouTube 자막 특화 | 아니요 | 일반 DOM text 범위를 넘어선 product-specific 기능 |
| ChatGPT chat/sidebar 동기화 | 아니요 | 이 프로젝트는 model/chat provider 독립 bridge |
| network/cookie/localStorage 추출 | 아니요 | 과도한 권한과 credential leakage 방지 |
| 임의 JavaScript 실행 | Full Access에서 예 | 호환성 escape hatch; 고위험으로 명시 |
| OS desktop app 제어 | 아니요 | 별도 native accessibility helper가 필요한 다른 trust boundary |

# Local Browser Bridge

브라우저 도구만 가진 AI 에이전트가 `localhost` 제어판을 통해 사용자가 허용한 실제 Chrome/Edge 탭을 읽고 조작하도록 연결하는 로컬 브리지입니다. Microsoft 365 Copilot Cowork처럼 **이 컴퓨터에서 실행되는 브라우저 자동화**가 `http://127.0.0.1:17373`을 열고, 대상 Chromium 확장은 outbound WebSocket으로 같은 서버에 연결합니다.

이 프로젝트는 OpenAI 내부 프로토콜을 복제하지 않습니다. 공식 문서에서 확인한 사용자 경험과 공개된 확장 플랫폼 패턴을 독립적으로 구현합니다. 조사 근거와 기능 비교는 [docs/RESEARCH.md](docs/RESEARCH.md)에 정리했습니다.

## 구현된 기능

- 로그인된 실제 Chromium 프로필의 허용된 탭 목록과 탭 전환
- URL 이동, 뒤로/앞으로, 새로고침, 새 탭
- 현재 뷰포트 스크린샷, 렌더링 텍스트, 선택 텍스트, 구조화된 interactive element refs
- ref 기반 클릭, 입력, `<select>` 선택, 스크롤, 제한된 키 입력
- 사이트 allowlist와 URL query/fragment 비공개 처리
- 삭제·전송·게시·구매·권한 변경·탭 닫기 계열의 **확장 팝업 human approval**
- password, 카드, OTP 계열 필드 자동 입력 차단
- loopback-only 서버, WebSocket token + extension Origin 검증, UI SameSite session + CSRF, CSP, no CORS
- 브라우저 에이전트 친화적인 접근성 DOM, 감사 로그, SSE 상태 갱신
- bearer-token REST 명령 API와 테스트용 mock extension

## 구조

```mermaid
flowchart LR
  A["M365 Copilot Cowork<br/>local Edge browser"] -->|"opens localhost UI"| B["Local control surface<br/>127.0.0.1:17373"]
  B --> C["Loopback Node server<br/>HTTP + SSE + command relay"]
  D["Target Chrome/Edge extension<br/>Manifest V3"] -->|"outbound token-auth WebSocket"| C
  C -->|"allowlisted commands"| D
  D --> E["User-approved browser tabs<br/>existing login/session"]
  E -->|"screenshot + DOM refs"| D
  F["Human"] -->|"approve sensitive action<br/>in extension popup"| D
```

중요한 네트워크 조건: Microsoft의 **local browser**처럼 에이전트 브라우저가 같은 PC에서 돌아야 합니다. Microsoft-hosted browser, Copilot Tasks cloud browser 등은 사용자 PC의 `127.0.0.1`에 접근할 수 없으므로 이 방식이 작동하지 않습니다.

## 설치와 실행

요구사항은 Node.js 20+와 Chrome 116+ 또는 호환 Edge입니다.

```bash
npm install
npm start
```

서버는 다음 정보를 출력합니다.

```text
Control surface: http://127.0.0.1:17373
Extension token: <random token>
```

그다음 대상 브라우저에 확장을 로드합니다.

1. Chrome에서 `chrome://extensions`를 엽니다. Edge라면 `edge://extensions`를 엽니다.
2. Developer mode를 켜고 **Load unpacked**를 선택합니다.
3. 이 저장소의 `extension/` 폴더를 선택합니다.
4. 확장 팝업을 열고 서버가 출력한 token과 포트 `17373`을 저장합니다.
5. 제어할 사이트를 연 뒤 확장 팝업에서 **Allow current site**를 선택합니다.

M365 Copilot Cowork에서는 local browser가 활성화된 Edge에서 다음처럼 요청할 수 있습니다.

```text
브라우저로 http://127.0.0.1:17373 을 열어. Browser Bridge의 지침에 따라
대상 탭을 관찰하고, [원하는 작업]을 수행해. 각 액션 뒤에는 다시 관찰해서
결과를 확인하고, human approval이 필요하면 멈춰서 나에게 알려줘.
```

민감한 액션이 감지되면 제어판은 실행하지 않고 대기합니다. 사람이 대상 브라우저의 Local Browser Bridge 팝업에서 **Approve once** 또는 **Reject**를 선택해야 합니다. 승인은 2분 후 만료됩니다.

## 빠른 로컬 데모

실제 확장을 설치하기 전에 UI와 통신만 확인할 수 있습니다. 터미널 두 개를 사용합니다.

```bash
LBB_TOKEN=demo-token npm start
```

```bash
LBB_TOKEN=demo-token npm run mock
```

`http://127.0.0.1:17373`을 열고 **Observe target**을 누르면 mock tab과 element refs가 표시됩니다. `/demo`에는 실제 확장 동작을 확인할 안전한 로컬 폼도 포함되어 있습니다.

## 환경 변수

| 변수 | 기본값 | 의미 |
|---|---|---|
| `LBB_PORT` | `17373` | loopback HTTP/WebSocket 포트 |
| `LBB_TOKEN` | 자동 생성 | 명시적 bridge token |
| `LBB_TOKEN_PATH` | `~/.local-browser-bridge/token` | 생성 token 저장 위치 |

서버는 코드상 `127.0.0.1`에만 bind하며 외부 인터페이스 bind를 거부합니다.

## REST 명령 API

브라우저 UI 이외의 로컬 클라이언트도 같은 명령을 사용할 수 있습니다.

```bash
curl http://127.0.0.1:17373/api/v1/command \
  -H "Authorization: Bearer $LBB_TOKEN" \
  -H "Content-Type: application/json" \
  --data '{"method":"tabs.list","params":{}}'
```

지원 명령과 WebSocket envelope는 [docs/PROTOCOL.md](docs/PROTOCOL.md)에 있습니다.

## 검증

```bash
npm run verify
```

검증 범위는 syntax, token persistence, allowlist 경계, URL redaction, risky-action 분류, sensitive-field 차단, CSRF/Origin 방어, WebSocket Origin 방어, command relay, screenshot 저장을 포함합니다.

## 제한사항

- 이것은 브라우저 computer-use bridge입니다. macOS/Windows의 다른 데스크톱 앱을 조작하는 native accessibility helper는 포함하지 않습니다.
- `chrome://`, `edge://`, 다른 확장 페이지, `file://`은 조작하지 않습니다.
- 사이트 구조가 바뀌면 ref가 만료됩니다. 반드시 다시 Observe한 뒤 새 ref를 사용해야 합니다.
- 클릭은 가능하면 Chrome DevTools Protocol의 trusted input을 사용하고 즉시 detach합니다. DevTools가 이미 해당 탭에 붙어 있으면 synthetic click으로 fallback하며 결과에 `trusted: false`가 표시됩니다.
- CAPTCHA, 로그인 비밀번호, 카드 데이터, OTP는 사람이 직접 처리해야 합니다.
- unpacked extension은 개발/개인 설치용입니다. 조직 배포에는 서명, 정책 검토, privacy disclosure가 별도로 필요합니다.

보안 모델과 위협 경계는 [SECURITY.md](SECURITY.md)를 확인하세요.

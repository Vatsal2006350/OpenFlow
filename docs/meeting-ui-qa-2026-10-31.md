# Meeting UI verification — October 31, 2026

The installed macOS app was exercised after rebuilding the UI and native shell.

| Flow | Evidence | Result |
| --- | --- | --- |
| Launch and Meetings library | Installed app, native screen inspection | Rendered; existing calendar accounts and notes retained |
| Record meeting | Real microphone capture, native screen inspection | Note opened with live timer and transcript |
| Minimize recording | Installed app | Native window became 410 × 190; all buttons visible |
| Open notes | Installed app | Full note restored; recording remained active |
| Discard | Installed app | Capture stopped, note hid, Hub recording state cleared; no note saved |
| Finish | Installed app, separate brief QA recording | Processing state rendered without freezing; note saved |
| Short recording | Installed app | Shows “Short recording · transcript saved”; does not claim generated summary |
| Transcript / Summary tabs | Installed app | Tabs switch; footer remains visible |
| Back to meetings | Installed app | Note hides and Hub shows saved result |
| Reminder layout and dismissal | Built web UI with mocked native event bridge | 410 × 190 viewport; footer bottom 175 px; native hide command used |
| Small window | Built web UI with mocked native event bridge | At 720 × 480, footer bottom 461 px; content scrolls independently |
| Generated summary, errors, processing, external saved event | Built web UI with fixture events | Correct phase and content; no JavaScript runtime errors in dismissal pass |
| Signing | Installed bundle verification and entitlement readback | Signature intact; audio-input and model runtime entitlements present |

The temporary QA meeting was removed by its exact ID after inspection. The two original notes were preserved, with a local backup retained. No live calendar invitation was created: reminder styling and state were checked with fixture events, and native compact-window sizing was checked through a real recording.

The final small change filters punctuation-only speech output before displaying or saving it. The final app is rebuilt and reloaded after that change.

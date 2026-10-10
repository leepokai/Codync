<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice. tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.12.1

### zh-Hant

- 重新整理記憶卡片：搜尋與篩選並排，新增、匯出、匯入等操作收進選單，每則記憶的動作改由右側選單開啟。

### en-US

- A cleaner Memory card: search and filter share one row, add, export and import live in a menu, and each memory's actions open from its own menu.

## 2.12.0

### zh-Hant

- 開發測試版本現在使用獨立的 App 與資料空間，可和正式版本並存。

- 遠端畫面會清楚指出電腦缺少的權限，並引導至電腦上的 Computer access 設定；僅有螢幕錄製權限時可先觀看。
- 記憶管理改用 Engram，可搜尋、新增、編輯、釘選及檢視記憶歷程；既有記憶會自動匯入。

- 電腦分組排序各裝置獨立保存；長按分組標題即可上下拖曳調整，不會影響其他手機或電腦。
- 修正重新連線或更新後，已在電腦刪除的 Bot 仍留在手機列表的問題。
- 改善確認視窗的全螢幕遮罩，讓頂部與底部在開關動畫中保持一致。
- 更新提示改用清楚、簡潔的圓形下載圖示。
- 憑證設定欄位保留清楚的標籤，並顯示尚未儲存連線憑證的狀態。
- 對話捲到底後繼續拖動時保留自然回彈，不再突然跳回底部或閃現跳至最新訊息按鈕。
- 改善跳至最新訊息的捲動動畫，避免長對話來回跳動，並可隨時用手勢中止。
- 調整對話氣泡與文字對比，讓閱讀更清楚。
- 對話列表摘要不再顯示粗體與程式碼的 Markdown 符號。

### en-US

- Development builds now use a separate app and data storage, so they can coexist with the production app.

- Remote screen explains missing computer permissions and guides you to Computer access setup. Watch in view-only mode when control is not allowed.
- Memory now uses Engram, with search, editing, pinning and history. Existing memories are imported automatically.

- Each device keeps its own computer group order. Touch and hold a group heading, then drag to reorder without changing your other devices.
- Bots deleted on your computer are removed correctly after reconnecting or updating.
- Confirmation dialogs dim the whole screen consistently, including the top and bottom during transitions.
- Update reminders use a clearer circular download icon.
- Credential fields keep their labels visible, with a clearer empty state for saved connections.
- Chats bounce naturally when dragged past the bottom, without snapping back or flashing the jump-to-latest button.
- Smoother jumps to the latest message, without back-and-forth movement in long chats; dragging interrupts immediately.
- Refined chat bubbles and text contrast for clearer reading.
- Conversation previews hide bold and code formatting markers.

## 2.10.0

### zh-Hant

- 開啟「使用電腦」的 bot 現在盡量在背景操作 app：直接按 app 裡的按鈕和欄位，不會搶走電腦上的滑鼠或把視窗拉到最前面。
- Windows 電腦上的 bot 現在也能使用電腦。

### en-US

- Bots with Use the computer on now work in apps in the background where they can: they press the app's own buttons and fields without taking the computer's pointer or bringing windows to the front.
- Bots on Windows computers can now use the computer too.

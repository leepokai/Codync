<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.40.0
### zh-Hant
- 全尺寸看圖可以雙指縮放（最多 8 倍）、放大後拖動，點兩下放大或還原。
- 看圖頁頂部新增「複製圖片」按鈕，會把圖片本身放進剪貼簿；bot 分享的圖片在關閉按鈕旁也能直接下載。
- 長按聊天裡的圖片，選單中的「複製」改為「複製圖片」和「複製名稱」；看圖時長按圖片也能使用回應和回覆。

### en-US
- Pinch to zoom a full-size picture up to 8x, drag it once zoomed in, or double-tap to zoom in and back.
- Copy image in the viewer puts the picture itself on the clipboard; a picture a bot shared also has Download by the close button.
- Holding a picture in the chat offers Copy image and Copy name in place of Copy; in the viewer, holding it also offers reactions and Reply.

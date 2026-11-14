; A new install says what usage analytics Codync shares and lets the user turn them off before
; anything is sent (docs/features/analytics.md). Updates and silent installs keep the setting.
!macro customInstall
  ${ifNot} ${isUpdated}
    MessageBox MB_YESNO|MB_ICONQUESTION "Codync sends usage analytics to PostHog to find bugs sooner: which features are used (with the app version, the platform and a bot's agent and model), crash and error reports, and an approximate location from your IP address, linked to your Codync account or a random ID. Never your messages, code, files or anything your agents produce.$\n$\nPrivacy policy: https://www.codync.dev/privacy$\n$\nShare usage analytics from this computer? You can change this later in Settings > General." /SD IDYES IDYES codyncAnalyticsChosen
    nsExec::Exec '"$INSTDIR\resources\codync-host.exe" analytics off'
    Pop $0
    ${if} $0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "Codync couldn't save this choice. Turn off Share usage analytics in Settings > General." /SD IDOK
    ${endIf}
    ; ponytail: a reinstall over an app whose host-prefs.json still says analytics on can report one
    ; host_setup_failed if the host never starts. Not patched from here: an installer that spawns
    ; PowerShell to edit files is what antivirus heuristics flag. Fix it in the app if it matters.
    codyncAnalyticsChosen:
  ${endIf}
!macroend

; Removing Codync takes its background host with it (the sign-in entry and the running host).
; An update reinstalls over the old version, and the new app starts the host again itself.
!macro customUnInstall
  ${ifNot} ${isUpdated}
    ExecWait '"$INSTDIR\resources\codync-host.exe" uninstall'
  ${endIf}
!macroend

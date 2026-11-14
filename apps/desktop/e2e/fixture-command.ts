/**
 * The command the desktop e2e uses to start a Python fixture as a custom bot. On Windows the host
 * starts it through `cmd.exe`: `python3` there is often the Store stub, and a single-quoted path
 * reaches Python with the quotes as part of the file name. So Windows gets `python` and double
 * quotes; macOS and Linux keep `python3` and single quotes.
 */
export function fixtureCommand(script: string, platform: string = process.platform): string {
  return platform === 'win32' ? `python -X utf8 -u "${script}"` : `python3 -X utf8 -u '${script}'`
}

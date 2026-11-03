/** The last component of a path, on either platform ("C:/Users/e/docs" → "docs"). */
export const lastPathComponent = (path: string) => path.split(/[/\\]/).filter(Boolean).pop() ?? path

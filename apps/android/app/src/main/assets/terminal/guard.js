'use strict';
window.addEventListener('error', () => CodyncTerminal.failed());
window.addEventListener('unhandledrejection', () => CodyncTerminal.failed());

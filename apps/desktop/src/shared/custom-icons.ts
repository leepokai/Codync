export interface CustomIcon {
  dataUrl: string
  tinted: boolean
}

export type CustomIcons = Record<string, CustomIcon>

/** Only host-normalized PNGs are rendered, never URLs supplied by another device. */
export function validIcon(icon: CustomIcon | undefined): icon is CustomIcon {
  return !!icon && typeof icon.dataUrl === 'string' && icon.dataUrl.length <= 65558
    && /^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(icon.dataUrl)
}

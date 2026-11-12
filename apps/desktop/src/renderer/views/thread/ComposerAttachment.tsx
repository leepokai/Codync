import { useEffect, useState } from 'react'
import { isImageName } from '@shared/models'
import { Icon } from '../../components/Icon'
import { Text } from '../../components/Text'
import type { OutgoingFile } from '../../store/outgoing-files'
import { useStore } from '../../store/context'
import { attachmentSymbol } from './Attachments'
import { useTr } from '../../lib/use-language'

/**
 * A local attachment, shown before it is sent: images as a whole-image thumbnail with the
 * remove button in the corner, anything else (or an image that won't decode) as a name chip.
 * While it uploads, a strip or a percentage says how much of it the computer has.
 */
export function ComposerAttachment({ file, remove }: { file: OutgoingFile; remove: () => void }) {
  const store = useStore()
  const percent = store.uploadPercents.get(file.id)
  const tr = useTr()
  const [url, setUrl] = useState<string | null>(null)
  const [broken, setBroken] = useState(false)

  useEffect(() => {
    if (!isImageName(file.name)) return
    const objectURL = URL.createObjectURL(new Blob([file.data as BlobPart]))
    setUrl(objectURL)
    return () => URL.revokeObjectURL(objectURL)
  }, [file])

  if (url && !broken) {
    return (
      <span className="attachment-tile" aria-label={tr('Image attachment: {name}').replaceAll('{name}', () => file.name)} title={file.name}>
        <img src={url} alt="" onError={() => setBroken(true)} />
        {percent !== undefined ? <span className="attachment-progress" style={{ width: `${percent}%` }} /> : null}
        <button className="press attachment-remove" aria-label={`Remove ${file.name}`} title="Remove" onClick={remove}>
          <Icon name="xmark" size={11} weight="bold" color="#fff" />
        </button>
      </span>
    )
  }
  return (
    <span className="file-chip">
      <Icon name={attachmentSymbol(file.name)} size={10} color="var(--secondary)" />
      <Text size="footnote" tone="text" truncate className="file-chip-name">{file.name}</Text>
      {percent !== undefined ? <Text size="caption2" tone="secondary" tabular>{`${percent}%`}</Text> : null}
      <button className="file-chip-remove" aria-label={`Remove ${file.name}`} title="Remove" onClick={remove}>
        <Icon name="xmark" size={10} weight="bold" color="var(--secondary)" />
      </button>
    </span>
  )
}

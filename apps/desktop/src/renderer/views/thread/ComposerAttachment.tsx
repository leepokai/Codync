import { useEffect, useState } from 'react'
import { isImageName } from '@shared/models'
import { Icon } from '../../components/Icon'
import { Text } from '../../components/Text'
import type { OutgoingFile } from '../../store/bot-store'
import { attachmentSymbol } from './Attachments'

/**
 * A local attachment, shown before it is sent: images as a whole-image thumbnail with the
 * remove button in the corner, anything else (or an image that won't decode) as a name chip.
 */
export function ComposerAttachment({ file, remove }: { file: OutgoingFile; remove: () => void }) {
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
      <span className="attachment-tile" aria-label={`Image attachment: ${file.name}`} title={file.name}>
        <img src={url} alt="" onError={() => setBroken(true)} />
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
      <button className="file-chip-remove" aria-label={`Remove ${file.name}`} title="Remove" onClick={remove}>
        <Icon name="xmark" size={10} weight="bold" color="var(--secondary)" />
      </button>
    </span>
  )
}

<script lang="ts">
  import { fileSrc, timeAgo, baseName, type Item } from './api'

  interface Props {
    item: Item
    index: number
    selected: boolean
    onselect: (id: number) => void
    onpin: (id: number) => void
    ondelete: (id: number) => void
  }

  let { item, index, selected, onselect, onpin, ondelete }: Props = $props()

  const kindLabel = $derived(
    item.kind === 'image' ? 'Image' : item.kind === 'files' ? 'Files' : item.has_rich ? 'Rich Text' : 'Text',
  )
</script>

<div
  class="card"
  class:selected
  role="option"
  aria-selected={selected}
  tabindex="-1"
  onclick={() => onselect(item.id)}
  onkeydown={(e) => e.key === 'Enter' && onselect(item.id)}
  oncontextmenu={(e) => {
    e.preventDefault()
    onpin(item.id)
  }}
>
  <div class="head">
    {#if item.source_icon_path}
      <img class="appicon" src={fileSrc(item.source_icon_path)} alt={item.source_app ?? ''} />
    {/if}
    <span class="kind">{kindLabel}</span>
    <span class="time">{timeAgo(item.last_copied_at)}</span>
    {#if index < 9}
      <span class="num">⌘{index + 1}</span>
    {/if}
    <span class="spacer"></span>
    {#if item.pinned}
      <span class="pin-flag" title="Pinned">●</span>
    {/if}
    <button
      class="act"
      title={item.pinned ? 'Unpin' : 'Pin'}
      onclick={(e) => {
        e.stopPropagation()
        onpin(item.id)
      }}>{item.pinned ? '⊖' : '⊕'}</button
    >
    <button
      class="act"
      title="Delete"
      onclick={(e) => {
        e.stopPropagation()
        ondelete(item.id)
      }}>✕</button
    >
  </div>

  <div class="body">
    {#if item.kind === 'image'}
      <img
        class="thumb"
        src={fileSrc(item.thumb_path ?? item.image_path ?? '')}
        alt="clipboard content"
        draggable="false"
      />
    {:else if item.kind === 'files'}
      <div class="files">
        <span class="file-glyph">📄</span>
        <div class="file-names">
          {#each (item.file_paths ?? []).slice(0, 2) as p (p)}
            <div class="file-name">{baseName(p)}</div>
          {/each}
          {#if (item.file_paths?.length ?? 0) > 2}
            <div class="file-more">+{(item.file_paths?.length ?? 0) - 2} more</div>
          {/if}
        </div>
      </div>
    {:else}
      <div class="text">{item.preview}</div>
    {/if}
  </div>
</div>

<style>
  .card {
    flex: none;
    width: 232px;
    display: flex;
    flex-direction: column;
    border-radius: 18px;
    background: var(--card-bg);
    box-shadow:
      inset 0 1px 0 var(--card-border),
      0 1px 6px rgba(0, 0, 0, 0.08);
    padding: 9px 11px 10px;
    transition:
      background 0.14s ease,
      transform 0.14s ease,
      box-shadow 0.14s ease;
    overflow: hidden;
  }

  .card:hover {
    background: var(--card-bg-hover);
  }

  .card.selected {
    background: var(--card-bg-hover);
    transform: translateY(-3px);
    box-shadow:
      inset 0 1px 0 var(--card-border),
      0 0 0 2px var(--accent),
      0 6px 18px rgba(10, 132, 255, 0.28);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10.5px;
    color: var(--text-dim);
    margin-bottom: 7px;
    flex: none;
  }

  .appicon {
    width: 15px;
    height: 15px;
    border-radius: 3.5px;
    flex: none;
  }

  .kind {
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .time,
  .num {
    font-variant-numeric: tabular-nums;
  }

  .num {
    opacity: 0;
    transition: opacity 0.12s;
  }

  .card.selected .num,
  .card:hover .num {
    opacity: 0.8;
  }

  .spacer {
    flex: 1;
  }

  .pin-flag {
    color: var(--accent);
    font-size: 7px;
  }

  .act {
    border: none;
    background: none;
    color: var(--text-dim);
    font-size: 11px;
    line-height: 1;
    padding: 2px 3px;
    border-radius: 5px;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.12s, color 0.12s;
  }

  .card:hover .act {
    opacity: 1;
  }

  .act:hover {
    color: var(--text);
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .text {
    font-size: 12px;
    line-height: 1.42;
    color: var(--text);
    white-space: pre-wrap;
    word-break: break-word;
    display: -webkit-box;
    -webkit-line-clamp: 6;
    line-clamp: 6;
    -webkit-box-orient: vertical;
    overflow: hidden;
    width: 100%;
  }

  .thumb {
    width: 100%;
    height: 100%;
    object-fit: cover;
    border-radius: 10px;
  }

  .files {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
  }

  .file-glyph {
    font-size: 26px;
    flex: none;
  }

  .file-names {
    min-width: 0;
  }

  .file-name {
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .file-more {
    font-size: 11px;
    color: var(--text-dim);
  }
</style>

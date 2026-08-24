<script lang="ts">
  import { onMount } from 'svelte'
  import ItemCard from './lib/ItemCard.svelte'
  import {
    axTrusted,
    clearAll,
    deleteItem,
    hideBar,
    listItems,
    onBarHiding,
    onBarShown,
    onItemsChanged,
    selectItem,
    togglePin,
    type Item,
  } from './lib/api'

  let items = $state<Item[]>([])
  let query = $state('')
  let selected = $state(0)
  let trusted = $state(true)
  let confirmingClear = $state(false)

  let searchEl: HTMLInputElement | undefined = $state()
  let rowEl: HTMLDivElement | undefined = $state()
  let confirmTimer: ReturnType<typeof setTimeout> | undefined

  async function refresh() {
    try {
      items = await listItems(query)
    } catch {
      items = []
    }
    if (selected >= items.length) selected = Math.max(0, items.length - 1)
  }

  // debounce type-to-filter round trips
  $effect(() => {
    void query
    const t = setTimeout(refresh, 60)
    return () => clearTimeout(t)
  })

  $effect(() => {
    rowEl?.children[selected]?.scrollIntoView({
      inline: 'nearest',
      block: 'nearest',
      behavior: 'smooth',
    })
  })

  onMount(() => {
    const subs = [
      onBarShown(async () => {
        query = ''
        selected = 0
        refresh()
        trusted = await axTrusted()
        requestAnimationFrame(() => searchEl?.focus())
      }),
      onBarHiding(() => {
        confirmingClear = false
      }),
      onItemsChanged(refresh),
    ]
    refresh()
    return () => subs.forEach((s) => s.then((un) => un()))
  })

  function pick(id: number) {
    selectItem(id)
    searchEl?.focus()
  }

  function pin(id: number) {
    togglePin(id)
  }

  function remove(id: number) {
    deleteItem(id)
  }

  function onClearClick() {
    if (confirmingClear) {
      clearAll()
      confirmingClear = false
      clearTimeout(confirmTimer)
    } else {
      confirmingClear = true
      clearTimeout(confirmTimer)
      confirmTimer = setTimeout(() => (confirmingClear = false), 2200)
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault()
      hideBar()
      return
    }
    if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
      e.preventDefault()
      const d = e.key === 'ArrowRight' ? 1 : -1
      selected = Math.min(Math.max(selected + d, 0), Math.max(items.length - 1, 0))
      return
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      const item = items[selected]
      if (item) pick(item.id)
      return
    }
    if (e.metaKey && e.key >= '1' && e.key <= '9') {
      e.preventDefault()
      const item = items[Number(e.key) - 1]
      if (item) pick(item.id)
      return
    }
    if (e.metaKey && e.key.toLowerCase() === 'p') {
      e.preventDefault()
      const item = items[selected]
      if (item) pin(item.id)
      return
    }
    if (e.metaKey && e.key === 'Backspace') {
      e.preventDefault()
      const item = items[selected]
      if (item) remove(item.id)
      return
    }
    // anything printable goes to the search field
    if (!e.metaKey && !e.ctrlKey && !e.altKey && searchEl && document.activeElement !== searchEl) {
      searchEl.focus()
    }
  }

  function onWheel(e: WheelEvent) {
    if (rowEl && Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
      e.preventDefault()
      rowEl.scrollLeft += e.deltaY
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<main class="bar">
  <header class="top">
    <svg class="search-icon" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="6.8" cy="6.8" r="4.9" stroke="currentColor" stroke-width="1.6" />
      <path d="M10.6 10.6 L14.3 14.3" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
    </svg>
    <input
      class="search"
      type="text"
      placeholder="Search clipboard…"
      spellcheck="false"
      autocomplete="off"
      bind:value={query}
      bind:this={searchEl}
    />
    {#if !trusted}
      <span class="ax-hint">Grant Accessibility for auto-paste</span>
    {/if}
    <span class="meta">{items.length} item{items.length === 1 ? '' : 's'}</span>
    <button class="clear-btn" class:confirming={confirmingClear} onclick={onClearClick}>
      {confirmingClear ? 'Really clear?' : 'Clear'}
    </button>
  </header>

  {#if items.length === 0}
    <div class="empty">{query ? 'No matches' : 'Nothing copied yet — copy something!'}</div>
  {:else}
    <div class="row" role="listbox" tabindex="-1" bind:this={rowEl} onwheel={onWheel}>
      {#each items as item, i (item.id)}
        <ItemCard
          {item}
          index={i}
          selected={i === selected}
          onselect={pick}
          onpin={pin}
          ondelete={remove}
        />
      {/each}
    </div>
  {/if}
</main>

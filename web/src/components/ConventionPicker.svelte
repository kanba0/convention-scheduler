<script lang="ts">
  import type { Convention } from '../api/types';

  let {
    conventions,
    selectedId,
    onselect,
  }: {
    conventions: Convention[];
    selectedId: string | null;
    onselect: (id: string) => void;
  } = $props();
</script>

<label class="picker">
  <span>convention</span>
  <select value={selectedId} onchange={(event) => onselect(event.currentTarget.value)}>
    {#each conventions as convention (convention.id)}
      <option value={convention.id}>{convention.name}</option>
    {/each}
  </select>
</label>

<style>
  .picker {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
  }

  span {
    color: var(--muted);
    font-size: 0.85rem;
  }

  select {
    font: inherit;
    padding: 0.3rem 0.5rem;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--surface);
    color: inherit;
  }
</style>
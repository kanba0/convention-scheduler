<script lang="ts">
  import { onMount } from 'svelte';

  import { listConventions } from './api/client';
  import type { Convention } from './api/types';
  import ConventionPicker from './components/ConventionPicker.svelte';

  let conventions = $state<Convention[]>([]);
  let selectedId = $state<string | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(true);

  const selected = $derived(conventions.find((c) => c.id === selectedId) ?? null);

  onMount(async () => {
    try {
      conventions = await listConventions();
      selectedId = conventions[0]?.id ?? null;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  });
</script>

<main>
  <h1>convention scheduler</h1>

  {#if loading}
    <p class="muted">loading…</p>
  {:else if error}
    <p class="error">could not reach the API: {error}</p>
  {:else if conventions.length === 0}
    <p class="muted">no conventions yet.</p>
  {:else}
    <ConventionPicker {conventions} {selectedId} onselect={(id) => (selectedId = id)} />

    {#if selected}
      <p class="muted">{selected.starts_on} → {selected.ends_on}</p>
    {/if}
  {/if}
</main>

<style>
  main {
    max-width: 60rem;
    margin: 0 auto;
    padding: 2rem 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    align-items: start;
  }

  h1 {
    font-size: 1.3rem;
    font-weight: 600;
    margin: 0;
  }

  .muted {
    color: var(--muted);
    margin: 0;
  }

  .error {
    color: var(--danger);
    margin: 0;
  }
</style>
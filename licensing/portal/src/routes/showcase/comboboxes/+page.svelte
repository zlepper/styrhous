<script>
  import SelectControl from '$lib/components/SelectControl.svelte';
  import '../showcase.css';
</script>

<svelte:head>
  <title>Select showcase · Styrhous</title>
</svelte:head>

<div class="component-showcase transparent-showcase select-showcase" data-testid="select-showcase">
  <label class="select-showcase-label" for="assigned-to-select">Assigned to</label>
  <SelectControl>
    <select id="assigned-to-select" aria-label="Assigned to">
      <option value="">Search...</option>
      <option>Michael Foster</option>
      <option>Floyd Miles</option>
      <option>Emily Selman</option>
      <option>Benjamin Russel</option>
    </select>
  </SelectControl>
</div>

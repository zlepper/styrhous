<script>
  import '../showcase.css';
</script>

<svelte:head>
  <title>Text input showcase · Styrhous</title>
</svelte:head>

<div class="component-showcase transparent-showcase text-input-showcase" data-testid="text-input-showcase">
  <label class="visually-hidden" for="showcase-metadata-key">Metadata key</label>
  <input
    id="showcase-metadata-key"
    class="input-size-native"
    type="text"
    placeholder="Enter a metadata key"
  />

  <label class="visually-hidden" for="showcase-column-header">Column header</label>
  <input
    id="showcase-column-header"
    class="input-size-native"
    type="text"
    value="Application"
  />
</div>

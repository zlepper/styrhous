<script lang="ts">
  import type { ActiveDevice } from '$lib/api';
  import { formatUtcDateTime } from '$lib/date';

  let { device, disabled = false, busy = false, onrevoke }:
    { device: ActiveDevice; disabled?: boolean; busy?: boolean;
      onrevoke: () => void } = $props();
</script>

<li class="device-row">
  <div class="device-row-main">
    <div>
      <h3>{device.displayName}</h3>
      <p>{device.platform} · Last seen <time datetime={device.lastSeenAt}>{formatUtcDateTime(device.lastSeenAt)}</time></p>
    </div>
    <button class="secondary-action danger-text" type="button" {disabled}
      aria-label={`Remove device ${device.displayName}`} onclick={onrevoke}>
      {busy ? 'Removing…' : 'Remove device'}
    </button>
  </div>
  <details class="device-details">
    <summary>Device details</summary>
    <dl class="record-facts">
      <div><dt>Architecture</dt><dd>{device.architecture}</dd></div>
      <div><dt>Styrhous version</dt><dd>{device.styrhousVersion}</dd></div>
      <div><dt>Added</dt><dd><time datetime={device.activatedAt}>{formatUtcDateTime(device.activatedAt)}</time></dd></div>
    </dl>
  </details>
</li>

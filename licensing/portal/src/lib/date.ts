const utcDateTime = new Intl.DateTimeFormat('en', {
  dateStyle: 'medium',
  timeStyle: 'short',
  timeZone: 'UTC'
});

export function formatUtcDateTime(value: string): string {
  return utcDateTime.format(new Date(value));
}

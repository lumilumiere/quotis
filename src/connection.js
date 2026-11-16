export function connectionBadge(provider, reading) {
  if (!provider.enabled) return { label:'Off', tone:'quiet' };
  if (reading?.error || reading?.status === 'error') return { label:'Issue', tone:'warn' };
  if (reading?.status === 'connected') return { label:'Connected', tone:'ok' };
  if (reading?.status === 'disabled') return { label:'Checking', tone:'warn' };
  if (reading?.status === 'not_detected') return { label:'Not found', tone:'warn' };
  return { label:provider.found ? 'Found' : 'Not found', tone:provider.found ? 'ok' : 'warn' };
}

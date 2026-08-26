import type { TicketSurface } from './ticket';
import table from './ticket-speaks.json';

/** Shared speaks table. Keep lockstep with `TicketSurface::speaks` in core. */
export const TICKET_SURFACE_SPEAKS: Record<TicketSurface, readonly string[]> = table;

export function ticketSurfaceSpeaks(surface: TicketSurface): string[] {
  return [...(TICKET_SURFACE_SPEAKS[surface] ?? [])];
}

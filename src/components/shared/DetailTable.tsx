import * as React from 'react';
import { cn } from '@/lib/utils';

const DetailTableContext = React.createContext(false);

export function useDetailTable(): boolean {
  return React.useContext(DetailTableContext);
}

/**
 * Quiet inspect-pane table. No header row — section titles already name the block.
 * Not TableShell: no card, resize, or row-open.
 */
export function DetailTable({
  className,
  children,
}: {
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <DetailTableContext.Provider value={true}>
      <table className={cn('w-full border-collapse text-body', className)} data-detail-table="">
        <tbody>{children}</tbody>
      </table>
    </DetailTableContext.Provider>
  );
}

export function DetailTableRow({
  label,
  children,
  className,
}: {
  label?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <tr className={cn('align-top', className)}>
      {label !== undefined ? (
        <th
          scope="row"
          className="w-[1%] whitespace-nowrap py-1 pr-3 text-left align-top text-meta font-normal text-muted"
        >
          {label}
        </th>
      ) : null}
      {children}
    </tr>
  );
}

export function DetailTableCell({
  children,
  className,
  colSpan,
}: {
  children?: React.ReactNode;
  className?: string;
  colSpan?: number;
}) {
  return (
    <td
      className={cn('min-w-0 py-1 align-top text-secondary', className)}
      colSpan={colSpan}
    >
      {children}
    </td>
  );
}

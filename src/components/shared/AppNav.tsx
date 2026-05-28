import { Link } from '@tanstack/react-router';
import { cn } from '@/lib/util';

const NAV_ITEMS = [
  { to: '/', label: 'Dashboard', exact: true },
  { to: '/transactions', label: 'Transactions', exact: false },
  { to: '/categories', label: 'Categories', exact: false },
  { to: '/account-summaries', label: 'Summaries', exact: false },
  { to: '/accounts', label: 'Accounts', exact: false },
  { to: '/import', label: 'Import', exact: false },
  { to: '/training', label: 'Training', exact: false },
  { to: '/settings', label: 'Settings', exact: false },
] as const;

const navLinkBase = cn(
  'relative flex items-center px-5 text-xs uppercase tracking-widest font-mono',
  'transition-all duration-200 select-none cursor-pointer',
  'text-muted-foreground hover:text-foreground',
  'after:absolute after:bottom-0 after:left-0 after:right-0 after:h-[2px]',
  'after:bg-accent after:scale-x-0 after:transition-transform after:duration-200 after:origin-left',
  'hover:after:scale-x-100 hover:bg-muted/60',
);

const navLinkActive = cn(
  'relative flex items-center px-5 text-xs uppercase tracking-widest font-mono',
  'transition-all duration-200 select-none cursor-pointer',
  'text-foreground',
  'after:absolute after:bottom-0 after:left-0 after:right-0 after:h-[2px]',
  'after:bg-accent after:scale-x-100 after:transition-transform after:duration-200 after:origin-left',
);

export const AppNav = (): React.JSX.Element => (
  <header
    className={cn(
      'sticky top-0 z-50 border-b border-border-muted',
      'bg-canvas-raised/80 backdrop-blur-sm',
      'shadow-[0_1px_0_0_var(--color-border-subtle),0_4px_16px_-4px_oklch(0%_0_0_/_0.06)]',
    )}
  >
    <div className="max-w-[1200px] mx-auto px-6 flex items-stretch h-14">
      <div className="flex items-center pr-8 border-r border-border-subtle shrink-0 gap-2.5">
        <span className="text-sm font-semibold tracking-tight font-sans">
          Julius
        </span>
        <span className="text-[10px] font-mono text-muted-foreground tracking-[0.2em] uppercase bg-muted px-1.5 py-0.5 border border-border-subtle">
          Wealth
        </span>
      </div>

      <nav className="flex items-stretch">
        {NAV_ITEMS.map(({ to, label, exact }) => (
          <Link
            key={to}
            to={to}
            activeOptions={{ exact }}
            className={navLinkBase}
            activeProps={{ className: navLinkActive }}
          >
            {label}
          </Link>
        ))}
        <Link
          to="/test"
          activeOptions={{ exact: true }}
          className={navLinkBase}
          activeProps={{ className: navLinkActive }}
        >
          Test
        </Link>
      </nav>
    </div>
  </header>
);

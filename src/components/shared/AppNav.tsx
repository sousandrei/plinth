import { Link, useRouterState } from '@tanstack/react-router';
import { logout } from '@/api/spaces';
import logoClean from '@/assets/logo-clean.svg';
import { Avatar } from '@/components/ui/Avatar';
import { useAuth } from '@/context/AuthContext';
import { useDemoMode } from '@/hooks/useDemoMode';
import { cn } from '@/lib/util';

const navLinkShared = cn(
  'relative flex items-center px-5 text-xs uppercase tracking-widest font-mono h-full gap-1.5 border-none bg-transparent',
  'transition-all duration-200 select-none cursor-pointer focus:outline-none',
  'after:absolute after:bottom-0 after:left-0 after:right-0 after:h-[2px]',
  'after:bg-accent after:transition-transform after:duration-200 after:origin-left',
);

const navLinkBase = cn(
  navLinkShared,
  'text-muted-foreground hover:text-foreground',
  'after:scale-x-0 hover:after:scale-x-100 hover:bg-muted/60',
);

const navLinkActive = cn(
  navLinkShared,
  'text-foreground bg-muted/30',
  'after:scale-x-100',
);

const dropdownItemBase = cn(
  'block px-4 py-2.5 text-xs font-mono uppercase tracking-wider text-muted-foreground hover:text-foreground no-underline',
  'hover:bg-muted/60 transition-colors duration-150',
);

const dropdownItemActive = cn(
  'block px-4 py-2.5 text-xs font-mono uppercase tracking-wider text-foreground bg-muted/60 font-semibold border-l-2 border-accent no-underline',
);

export const AppNav = (): React.JSX.Element => {
  const routerState = useRouterState();
  const currentPath = routerState.location.pathname;
  const { isDemoMode } = useDemoMode();
  const { user, setSpaceId, setUser } = useAuth();

  const isFinanceActive = [
    '/transactions',
    '/accounts',
    '/account-summaries',
  ].some((path) => currentPath.startsWith(path));
  const isManageActive = ['/categories', '/parsers', '/training'].some((path) =>
    currentPath.startsWith(path),
  );

  const handleLogout = () =>
    logout().then(() => {
      setSpaceId(null);
      setUser(null);
    });

  return (
    <>
      <header
        className={cn(
          'sticky top-0 z-50 border-b border-border-muted',
          'bg-canvas-raised/80 backdrop-blur-sm',
          'shadow-[0_1px_0_0_var(--color-border-subtle),0_4px_16px_-4px_oklch(0%_0_0_/_0.06)]',
        )}
      >
        <div className="max-w-[1200px] mx-auto px-6 flex items-stretch h-14 justify-between">
          <div className="flex items-stretch">
            <div className="flex items-center pr-8 border-r border-border-subtle shrink-0 gap-2.5 mr-2">
              <img src={logoClean} className="w-5 h-5 shrink-0" alt="" />
              <span className="text-sm font-semibold tracking-tight font-sans text-foreground">
                Plinth
              </span>
            </div>

            <nav className="flex items-stretch relative">
              {/* Dashboard */}
              <Link
                to="/"
                activeOptions={{ exact: true }}
                className={navLinkBase}
                activeProps={{ className: navLinkActive }}
              >
                Dashboard
              </Link>

              {/* Finance Dropdown */}
              <div className="relative h-full group">
                <button
                  type="button"
                  className={cn(navLinkBase, isFinanceActive && navLinkActive)}
                >
                  Finance
                  <svg
                    className="w-3 h-3 transition-transform duration-200 text-muted-foreground group-hover:rotate-180"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    strokeWidth={2}
                    role="img"
                    aria-label="Toggle Finance Menu"
                  >
                    <title>Toggle Finance Menu</title>
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M19 9l-7 7-7-7"
                    />
                  </svg>
                </button>

                <div className="absolute left-0 w-48 bg-canvas-raised border border-border-muted shadow-lg z-50 hidden group-hover:flex flex-col py-1 animate-in fade-in slide-in-from-top-1 duration-150">
                  <Link
                    to="/transactions"
                    className={
                      currentPath.startsWith('/transactions')
                        ? dropdownItemActive
                        : dropdownItemBase
                    }
                  >
                    Transactions
                  </Link>
                  <Link
                    to="/accounts"
                    className={
                      currentPath.startsWith('/accounts')
                        ? dropdownItemActive
                        : dropdownItemBase
                    }
                  >
                    Accounts
                  </Link>
                  <Link
                    to="/account-summaries"
                    className={
                      currentPath.startsWith('/account-summaries')
                        ? dropdownItemActive
                        : dropdownItemBase
                    }
                  >
                    Summaries
                  </Link>
                </div>
              </div>

              {/* Manage Dropdown */}
              <div className="relative h-full group">
                <button
                  type="button"
                  className={cn(navLinkBase, isManageActive && navLinkActive)}
                >
                  Manage
                  <svg
                    className="w-3 h-3 transition-transform duration-200 text-muted-foreground group-hover:rotate-180"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    strokeWidth={2}
                    role="img"
                    aria-label="Toggle Manage Menu"
                  >
                    <title>Toggle Manage Menu</title>
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M19 9l-7 7-7-7"
                    />
                  </svg>
                </button>

                <div className="absolute left-0 w-48 bg-canvas-raised border border-border-muted shadow-lg z-50 hidden group-hover:flex flex-col py-1 animate-in fade-in slide-in-from-top-1 duration-150">
                  <Link
                    to="/categories"
                    className={
                      currentPath.startsWith('/categories')
                        ? dropdownItemActive
                        : dropdownItemBase
                    }
                  >
                    Categories
                  </Link>
                  <Link
                    to="/parsers"
                    className={
                      currentPath.startsWith('/parsers')
                        ? dropdownItemActive
                        : dropdownItemBase
                    }
                  >
                    Parsers
                  </Link>
                  <Link
                    to="/training"
                    className={
                      currentPath.startsWith('/training')
                        ? dropdownItemActive
                        : dropdownItemBase
                    }
                  >
                    Training
                  </Link>
                </div>
              </div>
            </nav>
          </div>

          {/* Right — Avatar dropdown */}
          <div className="flex items-center">
            <div className="relative h-full group flex items-center">
              <Avatar name={user?.name ?? '?'} />

              <div className="absolute right-0 top-full w-52 bg-canvas-raised border border-border-muted shadow-lg z-50 hidden group-hover:flex flex-col py-1 animate-in fade-in slide-in-from-top-1 duration-150">
                {/* Identity header */}
                <div className="px-4 py-3 border-b border-border-subtle">
                  <p className="text-xs font-semibold text-foreground truncate">
                    {user?.name}
                  </p>
                </div>

                {/* Nav links */}
                <Link
                  to="/spaces"
                  className={cn(
                    dropdownItemBase,
                    currentPath.startsWith('/spaces') && dropdownItemActive,
                  )}
                >
                  Spaces
                </Link>
                <Link
                  to="/settings"
                  className={cn(
                    dropdownItemBase,
                    currentPath.startsWith('/settings') && dropdownItemActive,
                  )}
                >
                  Settings
                </Link>

                {/* Log out */}
                <div className="border-t border-border-subtle mt-1 pt-1">
                  <button
                    type="button"
                    onClick={handleLogout}
                    className={cn(dropdownItemBase, 'text-left w-full')}
                  >
                    Log out
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </header>

      {isDemoMode && (
        <div className="sticky top-14 z-40 flex items-center justify-center gap-2 bg-amber-500/10 border-b border-amber-500/30 px-4 py-1.5">
          <span className="text-amber-400 text-[10px]">⚠</span>
          <span className="text-[10px] font-mono uppercase tracking-widest text-amber-400/90">
            Demo mode — data shown is not real
          </span>
        </div>
      )}
    </>
  );
};

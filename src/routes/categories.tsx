import { Tag, Warning } from '@phosphor-icons/react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import type React from 'react';

import {
  createCategory,
  deleteCategory,
  listAllCategories,
  updateCategory,
} from '@/api/categories';
import { CategoryForm } from '@/components/categories/CategoryForm';
import { CategoryRow } from '@/components/categories/CategoryRow';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { updateCategoryColors } from '@/lib/category-color';

export const Route = createFileRoute('/categories')({
  component: CategoriesPage,
});

function CategoriesPage(): React.JSX.Element {
  const queryClient = useQueryClient();

  // 1. Fetch categories
  const {
    data: categories = [],
    isLoading,
    error,
  } = useQuery({
    queryKey: ['categories'],
    queryFn: listAllCategories,
  });

  // Dynamically update colors mapping
  if (categories.length > 0) {
    updateCategoryColors(categories);
  }

  // 2. Create category mutation
  const createMutation = useMutation({
    mutationFn: ({ name, color }: { name: string; color: string }) =>
      createCategory(name, color),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['categories'] });
    },
    onError: (err: unknown) => {
      alert(
        `Failed to create category: ${err instanceof Error ? err.message : String(err)}`,
      );
    },
  });

  // 3. Delete category mutation
  const deleteMutation = useMutation({
    mutationFn: deleteCategory,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['categories'] });
      queryClient.invalidateQueries({ queryKey: ['transactions'] });
      queryClient.invalidateQueries({ queryKey: ['aggregations'] });
    },
    onError: (err: unknown) => {
      alert(
        `Failed to delete category: ${err instanceof Error ? err.message : String(err)}`,
      );
    },
  });

  // 4. Update category mutation
  const updateMutation = useMutation({
    mutationFn: ({
      id,
      name,
      color,
    }: {
      id: string;
      name: string;
      color: string;
    }) => updateCategory(id, name, color),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['categories'] });
      queryClient.invalidateQueries({ queryKey: ['transactions'] });
      queryClient.invalidateQueries({ queryKey: ['aggregations'] });
    },
    onError: (err: unknown) => {
      alert(
        `Failed to update category: ${err instanceof Error ? err.message : String(err)}`,
      );
    },
  });

  const handleCreateSubmit = (name: string, color: string) => {
    createMutation.mutate({ name, color });
  };

  const handleDelete = (id: string, name: string) => {
    const confirmed = window.confirm(
      `Are you sure you want to delete the category "${name}"?\n\n` +
        `This will remove the category from all transactions referencing it, setting them to "Uncategorized".`,
    );
    if (confirmed) {
      deleteMutation.mutate(id);
    }
  };

  const handleUpdate = async (id: string, name: string, color: string) => {
    await updateMutation.mutateAsync({ id, name, color });
  };

  return (
    <div className="max-w-[1200px] mx-auto px-6 py-10 flex flex-col gap-8">
      {/* Page Header */}
      <div className="flex flex-col gap-1 animate-fade-in text-left">
        <h1 className="text-2xl font-semibold tracking-tight">Categories</h1>
        <p className="text-xs font-mono text-muted-foreground uppercase tracking-widest">
          Manage transaction categories for ML training and dashboard
          aggregations
        </p>
      </div>

      {/* Main Layout Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-8 items-start animate-fade-in">
        {/* Sidebar: Add Category */}
        <div className="md:col-span-1">
          <Card>
            <CardHeader label="Category Manager" meta="Add New" />
            <CardBody>
              <CategoryForm
                isPending={createMutation.isPending}
                onSubmit={handleCreateSubmit}
              />
            </CardBody>
          </Card>
        </div>

        {/* Main List: View Categories */}
        <div className="md:col-span-2 flex flex-col gap-4">
          <Card>
            <CardHeader
              label="Registered Categories"
              meta={`${categories.length} Total`}
            />
            <CardBody className="p-0">
              {isLoading ? (
                <div className="flex flex-col items-center justify-center py-16 gap-3">
                  <Spinner size="md" />
                  <span className="text-xs font-mono tracking-widest uppercase text-muted-foreground">
                    Fetching categories…
                  </span>
                </div>
              ) : error ? (
                <div className="flex flex-col items-center justify-center py-12 px-6 gap-2 text-center text-rose-500">
                  <Warning size={32} />
                  <span className="text-sm font-medium">
                    Failed to load categories
                  </span>
                  <span className="text-xs font-mono text-muted-foreground">
                    {String(error)}
                  </span>
                </div>
              ) : categories.length === 0 ? (
                <div className="flex flex-col items-center justify-center py-16 px-6 gap-2 text-center">
                  <Tag size={36} className="text-muted-foreground opacity-50" />
                  <span className="text-sm font-medium">
                    No categories registered
                  </span>
                  <span className="text-xs text-muted-foreground max-w-sm">
                    Add standard or custom categories using the manager panel on
                    the left to start classifying transactions.
                  </span>
                </div>
              ) : (
                <div className="divide-y divide-border-subtle">
                  {categories.map((cat) => (
                    <CategoryRow
                      key={cat.id}
                      cat={cat}
                      isDeletePending={deleteMutation.isPending}
                      isUpdatePending={updateMutation.isPending}
                      onDelete={handleDelete}
                      onUpdate={handleUpdate}
                    />
                  ))}
                </div>
              )}
            </CardBody>
          </Card>
        </div>
      </div>
    </div>
  );
}

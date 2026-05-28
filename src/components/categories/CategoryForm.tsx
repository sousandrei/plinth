import { Palette, Plus } from '@phosphor-icons/react';
import type React from 'react';
import { useState } from 'react';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Spinner } from '@/components/ui/Spinner';

const PRESET_COLORS = [
  '#22c55e', // Green
  '#10b981', // Emerald
  '#06b6d4', // Cyan
  '#3b82f6', // Blue
  '#6366f1', // Indigo
  '#f97316', // Orange
  '#ec4899', // Pink
  '#8b5cf6', // Purple
  '#eab308', // Yellow
  '#ef4444', // Red
  '#6b7280', // Gray
];

interface CategoryFormProps {
  isPending: boolean;
  onSubmit: (name: string, color: string) => void;
}

export const CategoryForm = ({
  isPending,
  onSubmit,
}: CategoryFormProps): React.JSX.Element => {
  const [newCategoryName, setNewCategoryName] = useState('');
  const [newCategoryColor, setNewCategoryColor] = useState('#3b82f6');

  const handleCreate = (e: React.FormEvent) => {
    e.preventDefault();
    if (!newCategoryName.trim()) return;
    onSubmit(newCategoryName.trim(), newCategoryColor);
    setNewCategoryName('');
    setNewCategoryColor('#3b82f6');
  };

  return (
    <form onSubmit={handleCreate} className="flex flex-col gap-5 text-left">
      <div className="flex flex-col gap-1.5">
        <label
          htmlFor="category-name"
          className="text-xs font-mono uppercase tracking-widest text-muted-foreground"
        >
          Category Name
        </label>
        <Input
          id="category-name"
          placeholder="e.g. Entertainment"
          value={newCategoryName}
          onChange={(e) => setNewCategoryName(e.target.value)}
          disabled={isPending}
          required
          className="w-full"
        />
      </div>

      <div className="flex flex-col gap-2">
        <span className="text-xs font-mono uppercase tracking-widest text-muted-foreground">
          Theme Color
        </span>
        <div className="flex flex-wrap gap-2 items-center">
          {PRESET_COLORS.map((preset) => (
            <button
              key={preset}
              type="button"
              onClick={() => setNewCategoryColor(preset)}
              className={`w-6 h-6 rounded-full border-2 transition-all cursor-pointer ${
                newCategoryColor === preset
                  ? 'border-foreground scale-110'
                  : 'border-transparent hover:scale-105'
              }`}
              style={{ backgroundColor: preset }}
            />
          ))}
          <div className="relative w-7 h-7 flex items-center justify-center rounded-full border border-border overflow-hidden cursor-pointer hover:border-foreground transition-all">
            <input
              type="color"
              value={newCategoryColor}
              onChange={(e) => setNewCategoryColor(e.target.value)}
              className="absolute inset-0 w-full h-full p-0 border-0 cursor-pointer scale-125 opacity-0 z-10"
            />
            <Palette
              size={13}
              className="text-muted-foreground z-0 pointer-events-none"
            />
          </div>
        </div>
      </div>

      <Button
        type="submit"
        disabled={isPending || !newCategoryName.trim()}
        className="w-full h-10 gap-2 font-mono uppercase tracking-widest text-xs mt-2"
      >
        {isPending ? <Spinner size="sm" /> : <Plus size={16} />}
        Add Category
      </Button>
    </form>
  );
};

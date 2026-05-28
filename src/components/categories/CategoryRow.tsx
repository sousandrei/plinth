import { Check, Palette, PencilSimple, Trash, X } from '@phosphor-icons/react';
import type React from 'react';
import { useState } from 'react';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Spinner } from '@/components/ui/Spinner';
import { categoryChipStyle } from '@/lib/category-color';
import type { Category } from '@/types';

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

interface CategoryRowProps {
  cat: Category;
  isDeletePending: boolean;
  isUpdatePending: boolean;
  onDelete: (id: string, name: string) => void;
  onUpdate: (id: string, name: string, color: string) => Promise<void> | void;
}

export const CategoryRow = ({
  cat,
  isDeletePending,
  isUpdatePending,
  onDelete,
  onUpdate,
}: CategoryRowProps): React.JSX.Element => {
  const [isEditing, setIsEditing] = useState(false);
  const [editingName, setEditingName] = useState(cat.name);
  const [editingColor, setEditingColor] = useState(cat.color);

  const startEdit = () => {
    setEditingName(cat.name);
    setEditingColor(cat.color);
    setIsEditing(true);
  };

  const cancelEdit = () => {
    setIsEditing(false);
  };

  const handleSave = async () => {
    if (!editingName.trim() || !editingColor.trim()) return;
    await onUpdate(cat.id, editingName.trim(), editingColor);
    setIsEditing(false);
  };

  return (
    <div className="px-6 py-4 flex items-center justify-between hover:bg-muted/30 transition-colors duration-150 min-h-[64px]">
      {isEditing ? (
        <div className="flex items-center gap-3 w-full">
          <Input
            value={editingName}
            onChange={(e) => setEditingName(e.target.value)}
            disabled={isUpdatePending}
            className="h-8 max-w-xs font-medium text-xs py-1 px-2"
            required
          />
          <div className="flex items-center gap-1.5">
            {PRESET_COLORS.map((preset) => (
              <button
                key={preset}
                type="button"
                onClick={() => setEditingColor(preset)}
                className={`w-5 h-5 rounded-full border transition-all cursor-pointer ${
                  editingColor === preset
                    ? 'border-foreground scale-110'
                    : 'border-transparent hover:scale-105'
                }`}
                style={{ backgroundColor: preset }}
              />
            ))}
            <div className="relative w-6 h-6 flex items-center justify-center rounded-full border border-border overflow-hidden cursor-pointer hover:border-foreground transition-all ml-0.5">
              <input
                type="color"
                value={editingColor}
                onChange={(e) => setEditingColor(e.target.value)}
                className="absolute inset-0 w-full h-full p-0 border-0 cursor-pointer scale-125 opacity-0 z-10"
              />
              <Palette
                size={11}
                className="text-muted-foreground z-0 pointer-events-none"
              />
            </div>
          </div>
          <div className="flex items-center gap-1.5 ml-auto">
            <Button
              variant="ghost"
              disabled={isUpdatePending}
              onClick={handleSave}
              className="p-2 text-emerald-500 hover:text-emerald-600 hover:bg-emerald-500/10 active:scale-[0.95]"
              title="Save Changes"
            >
              {isUpdatePending ? <Spinner size="sm" /> : <Check size={16} />}
            </Button>
            <Button
              variant="ghost"
              disabled={isUpdatePending}
              onClick={cancelEdit}
              className="p-2 text-muted-foreground hover:text-foreground hover:bg-muted/30 active:scale-[0.95]"
              title="Cancel"
            >
              <X size={16} />
            </Button>
          </div>
        </div>
      ) : (
        <>
          <div className="flex items-center gap-3">
            <span
              className="px-2 py-0.5 text-xs font-medium border rounded-sm tracking-wide"
              style={categoryChipStyle(cat.name)}
            >
              {cat.name}
            </span>
          </div>

          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              onClick={startEdit}
              disabled={isDeletePending || isUpdatePending}
              className="p-2 text-muted-foreground hover:text-foreground hover:bg-muted/30 active:scale-[0.95]"
              title="Edit Category"
            >
              <PencilSimple size={16} />
            </Button>

            {/* We prevent deleting the default catch-all "Other" category to keep training loops fully stable */}
            {cat.name !== 'Other' ? (
              <Button
                variant="ghost"
                onClick={() => onDelete(cat.id, cat.name)}
                disabled={isDeletePending || isUpdatePending}
                className="p-2 text-rose-500 hover:text-rose-600 hover:bg-rose-500/10 active:scale-[0.95]"
                title="Delete Category"
              >
                {isDeletePending ? <Spinner size="sm" /> : <Trash size={16} />}
              </Button>
            ) : (
              <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest px-2 select-none">
                System Default
              </span>
            )}
          </div>
        </>
      )}
    </div>
  );
};

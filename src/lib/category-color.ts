// Dynamic map populated from the database categories
const categoryColorMap: Record<string, string> = {};

/**
 * Updates the local color map with the categories loaded from the database.
 */
export const updateCategoryColors = (
  categories: { name: string; color: string }[],
) => {
  for (const cat of categories) {
    categoryColorMap[cat.name] = cat.color;
  }
};

const FALLBACK = '#6b7280';

/**
 * Returns a Hex color string for a given category name.
 */
export const categoryColor = (category: string): string => {
  return categoryColorMap[category] ?? FALLBACK;
};

/**
 * Returns an inline style object `{ color, borderColor, backgroundColor }`
 * suitable for a category chip using hex transparency.
 */
export const categoryChipStyle = (
  category: string,
): { color: string; borderColor: string; backgroundColor: string } => {
  const color = categoryColor(category);
  return {
    color,
    borderColor: color,
    backgroundColor: `${color}15`, // 8% opacity in hex format
  };
};

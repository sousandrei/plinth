import { categoryColor } from '@/lib/category-color';
import type { Aggregations } from '@/types';
import { type CategoryMeta, type SpendingPoint, toMajor } from './types';

export function deriveSpending(
  aggregations: Aggregations,
  months: string[],
): { series: SpendingPoint[]; categories: CategoryMeta[] } {
  // Only show the most recent 12 months
  const window = months.slice(-12);

  const categorySet = new Set<string>();
  for (const month of window) {
    for (const cat of Object.keys(aggregations[month].by_category)) {
      categorySet.add(cat);
    }
  }
  const categoryNames = Array.from(categorySet).sort();

  const series: SpendingPoint[] = window.map((month) => {
    const cats = aggregations[month].by_category;
    const point: SpendingPoint = { month };
    for (const cat of categoryNames) {
      // Absolute value — chart shows magnitude of spend, not sign
      point[cat] = toMajor(Math.abs(cats[cat] ?? 0));
    }
    return point;
  });

  const categories: CategoryMeta[] = categoryNames.map((name) => ({
    name,
    color: categoryColor(name),
  }));

  return { series, categories };
}

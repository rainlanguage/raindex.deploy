// Stub for SvelteKit's `$app/navigation`. No-op navigation so components that
// call goto()/invalidate() render without a real router.
export const goto = async () => {};
export const invalidate = async () => {};
export const invalidateAll = async () => {};
export const beforeNavigate = () => {};
export const afterNavigate = () => {};
export const preloadData = async () => {};
export const preloadCode = async () => {};
export const pushState = () => {};
export const replaceState = () => {};

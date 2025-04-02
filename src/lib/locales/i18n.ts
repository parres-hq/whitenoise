import { getLocaleFromNavigator, init, register } from "svelte-i18n";

register("en", () => import("./en.json"));
register("es", () => import("./es.json"));

export async function initI18n() {
    return init({
        fallbackLocale: "en",
        initialLocale: getLocaleFromNavigator(),
    });
}

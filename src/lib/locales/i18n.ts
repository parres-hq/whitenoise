import { getLocaleFromNavigator, init, register } from "svelte-i18n";

register("en", () => import("./en.json"));
register("es", () => import("./es.json"));
register("it", () => import("./it.json"));
register("de", () => import("./de.json"));

export async function initI18n() {
    return init({
        fallbackLocale: "en",
        initialLocale: getLocaleFromNavigator(),
    });
}

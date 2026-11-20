//! Reading the mod's own recipe types, `create:pressing` and `create:milling`: data files with an
//! `ingredient`, a `result` and an optional `work`, read through the game's recipe table.
use crate::pressing::work_of;
use mod_api::{Host, ItemStack};

/// The first recipe of type `kind` whose ingredient takes `stack`, as its result and the work it
/// needs (the recipe's `work`, or the default).
pub fn matching(host: &dyn Host, kind: &str, stack: &ItemStack) -> Option<(ItemStack, f32)> {
    let recipes = host.recipes_of_type(kind);
    for recipe in recipes.iter() {
        let Some(ingredient) = recipe.json.get("ingredient") else {
            continue;
        };
        let Ok(Some(ingredient)) = host.recipe_ingredient(&ingredient.to_string()) else {
            continue;
        };
        if !ingredient.test(host, stack) {
            continue;
        }
        let Some(result) = recipe.json.get("result") else {
            continue;
        };
        let Ok(Some(result)) = host.recipe_result(&result.to_string()) else {
            continue;
        };
        let work = work_of(recipe.json.get("work").and_then(|work| work.as_f64()));
        return Some((result, work));
    }
    None
}

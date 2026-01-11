//! Pagination component

use leptos::prelude::*;

#[component]
pub fn Pagination(
    #[prop(into)] current_page: Signal<usize>,
    #[prop(into)] total_pages: Signal<usize>,
    #[prop(into)] total_items: Signal<usize>,
    #[prop(into)] items_per_page: Signal<usize>,
    #[prop(into)] on_page_change: Callback<usize>,
    #[prop(into)] on_page_size_change: Callback<usize>,
) -> impl IntoView {
    let page_sizes = vec![10, 20, 50, 100];
    
    let can_prev = move || current_page.get() > 1;
    let can_next = move || current_page.get() < total_pages.get();
    
    let on_prev = move |_| {
        if can_prev() {
            on_page_change.run(current_page.get() - 1);
        }
    };
    
    let on_next = move |_| {
        if can_next() {
            on_page_change.run(current_page.get() + 1);
        }
    };
    
    // Generate visible page numbers
    let visible_pages = move || {
        let current = current_page.get();
        let total = total_pages.get();
        let mut pages = Vec::new();
        
        if total <= 7 {
            pages.extend(1..=total);
        } else {
            pages.push(1);
            
            if current > 3 {
                pages.push(0); // ellipsis marker
            }
            
            let start = current.saturating_sub(1).max(2);
            let end = (current + 1).min(total - 1);
            
            for p in start..=end {
                if !pages.contains(&p) {
                    pages.push(p);
                }
            }
            
            if current < total - 2 {
                pages.push(0); // ellipsis marker
            }
            
            if !pages.contains(&total) {
                pages.push(total);
            }
        }
        
        pages
    };

    view! {
        <div class="pagination">
            <div class="pagination-info">
                <span class="pagination-total">
                    {move || format!("{} items", total_items.get())}
                </span>
                
                <select 
                    class="pagination-size"
                    prop:value=move || items_per_page.get().to_string()
                    on:change=move |ev| {
                        if let Ok(size) = event_target_value(&ev).parse::<usize>() {
                            on_page_size_change.run(size);
                        }
                    }
                >
                    {page_sizes.iter().map(|&size| {
                        view! {
                            <option value=size.to_string()>{size}" / page"</option>
                        }
                    }).collect_view()}
                </select>
            </div>
            
            <div class="pagination-controls">
                <button 
                    class="pagination-btn"
                    disabled=move || !can_prev()
                    on:click=on_prev
                >
                    "‹"
                </button>
                
                {move || {
                    visible_pages().into_iter().map(|page| {
                        if page == 0 {
                            view! { <span class="pagination-ellipsis">"..."</span> }.into_any()
                        } else {
                            let is_current = current_page.get() == page;
                            view! {
                                <button 
                                    class=if is_current { "pagination-btn active" } else { "pagination-btn" }
                                    on:click=move |_| on_page_change.run(page)
                                >
                                    {page}
                                </button>
                            }.into_any()
                        }
                    }).collect_view()
                }}
                
                <button 
                    class="pagination-btn"
                    disabled=move || !can_next()
                    on:click=on_next
                >
                    "›"
                </button>
            </div>
        </div>
    }
}

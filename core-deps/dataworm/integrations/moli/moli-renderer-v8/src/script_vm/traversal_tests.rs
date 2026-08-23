use super::ScriptVmDefaultWorldBootstrap;
use super::StandaloneScriptVmHarness;
use crate::dom::native::{DomHost, NativeDom};

fn new_vm() -> StandaloneScriptVmHarness {
    let _js_runtime = crate::JsRuntime::initialize();
    let page_task_queue = crate::page_task_queue::PageTaskQueueTestHarness::new();
    let post_domcontentloaded_page_task_sender =
        page_task_queue.owner_attached_runtime_page_task_sender_for_test();
    let page_task_front_injection_tx = page_task_queue.parser_boundary_sender();
    ScriptVmDefaultWorldBootstrap::standalone_from_dom_host_for_test(
        DomHost::from_dom(NativeDom::new(
            url::Url::parse("https://traversal.test/").expect("test url"),
        )),
        post_domcontentloaded_page_task_sender,
        page_task_front_injection_tx,
    )
    .expect("script vm bootstrap should succeed")
    .finish()
    .expect("script vm finish should succeed")
}

fn eval(script: &str) -> String {
    let mut vm = new_vm();
    vm.eval(script).expect("script should evaluate")
}

#[test]
fn tree_walker_sibling_climbs_past_skipped_parent() {
    let status = eval(
        r#"
        (() => {
            const root = document.createElement('div');
            const left = document.createElement('section');
            const middle = document.createElement('section');
            const right = document.createElement('section');
            const leftChild = document.createElement('p');
            const middleChild = document.createElement('p');
            const rightChild = document.createElement('p');
            leftChild.id = 'left-child';
            middleChild.id = 'middle-child';
            rightChild.id = 'right-child';
            left.appendChild(leftChild);
            middle.appendChild(middleChild);
            right.appendChild(rightChild);
            root.appendChild(left);
            root.appendChild(middle);
            root.appendChild(right);

            const walker = document.createTreeWalker(
                root,
                NodeFilter.SHOW_ELEMENT,
                {
                    acceptNode(node) {
                        if (node.localName === 'section') {
                            return NodeFilter.FILTER_SKIP;
                        }
                        return NodeFilter.FILTER_ACCEPT;
                    }
                }
            );

            walker.currentNode = middleChild;
            const next = walker.nextSibling();
            walker.currentNode = middleChild;
            const previous = walker.previousSibling();

            return [
                next && next.id,
                previous && previous.id
            ].join('|');
        })()
        "#,
    );
    assert_eq!(status, "right-child|left-child");
}

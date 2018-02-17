//! Document + versioning state that talks to a synchronization server.

use self::actions::*;
use failure::Error;
use oatie::OT;
use oatie::doc::*;
use oatie::schema::RtfSchema;
use rand;
use rand::Rng;
use std::{panic, process};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::mem;
use super::*;
use oatie::validate::validate_doc;
use colored::Colorize;

#[derive(Debug)]
pub struct ClientDoc {
    pub doc: Doc,
    pub version: usize,

    pub original_doc: Doc,
    pub pending_op: Option<Op>,
    pub local_op: Op,
}

impl ClientDoc {
    // Default
    pub fn new() -> ClientDoc {
        ClientDoc {
            doc: Doc(vec![]),
            version: 100,

            original_doc: Doc(vec![]),
            pending_op: None,
            local_op: Op::empty(),
        }
    }

    /// Sync ACK'd our pending operation.
    /// Returns the next op to send to sync, if any.
    // TODO we can determine new_doc without needing it passed in
    pub fn sync_confirmed_pending_op(&mut self, new_doc: &Doc, version: usize) -> Option<Op> {
        log_wasm!(SyncNew("confirmed_pending_op".into()));

        // Server can't acknowledge an operation that wasn't pending.
        assert!(self.pending_op.is_some());
        // Likewise, the new doc update should be equivalent to original_doc : pending_op 
        // or otherwise server acknowledged something improper.
        println!("pending {:?}", self.pending_op);
        assert_eq!(
            new_doc, 
            &OT::apply(&self.original_doc, self.pending_op.as_ref().unwrap()),
            "invalid ack from Sync"
        );

        self.original_doc = new_doc.clone();

        // Reassemble local document.
        self.doc = OT::apply(new_doc, &self.local_op);
        self.version = version;

        validate_doc(&self.doc).expect("Validation error after pending op");

        // Now that we have an ack, we can send up our new ops.
        self.pending_op = None;
        self.next_payload()
    }

    /// Sync gave us an operation not originating from us.
    // TODO we can determine new_doc without needing it passed in
    pub fn sync_sent_new_version(&mut self, new_doc: &Doc, version: usize, input_op: &Op) {
        // log_wasm!(SyncNew("new_op".into()));
        self.assert_compose_correctness();

        // Optimization
        if self.pending_op.is_none() && self.local_op == Op::empty() {
            // Skip ahead
            self.doc = new_doc.clone();
            self.version = version;
            self.original_doc = new_doc.clone();
            return;
        }
        
        println!("\n----> TRANSFORMING");

        // Extract the pending op.
        assert!(self.pending_op.is_some());
        let pending_op = self.pending_op.clone().unwrap();

        // Extract and compose all local ops.
        let local_op = self.local_op.clone();

        // Transform.
        println!();
        println!("<test>");
        println!("server: {:?}", input_op);
        println!();
        println!("pending: {:?}", pending_op);
        println!("client: {:?}", local_op);
        println!("</test>");
        println!();

        // I x P -> I', P'
        let (pending_op_transform, input_op_transform) = Op::transform::<RtfSchema>(&input_op, &pending_op);
        // P' x L -> P'', L'
        let (local_op_transform, _) = Op::transform::<RtfSchema>(&input_op_transform, &local_op);

        // client_doc = input_doc : I' : P''
        let client_op = Op::compose(&pending_op_transform, &local_op_transform);
        // Reattach to doc.
        self.doc = Op::apply(&new_doc, &pending_op_transform);
        validate_doc(&self.doc).expect("Validation error after unrelated pending op");
        self.doc = Op::apply(&self.doc, &local_op_transform);

        println!();
        println!("<test>");
        println!("pending_op: {:?}", pending_op);
        println!();
        println!("local_op: {:?}", local_op);
        println!();
        println!("input_op: {:?}", input_op);
        println!();
        println!("new_doc: {:?}", new_doc);
        println!();
        println!("pending_op_transform: {:?}", pending_op_transform);
        println!();
        println!("new_doc_pending: {:?}", Op::apply(&new_doc, &pending_op_transform));
        println!();
        println!("local_op_transform: {:?}", local_op_transform);
        println!("</test>");
        println!();

        validate_doc(&self.doc).expect("Validation error after unrelated op");

        let mirror = Op::apply(&new_doc, &Op::compose(&pending_op_transform, &local_op_transform));
        assert_eq!(self.doc, mirror);

        // Set pending and local ops.
        self.pending_op = Some(pending_op_transform);
        self.local_op = local_op_transform;   

        // Update other variables.
        self.version = version;
        self.original_doc = new_doc.clone();

        println!("{}", format!("\n----> result {:?}\n{:?}\n{:?}\n\n{:?}\n\n", self.original_doc, self.pending_op, self.local_op, self.doc).red());

        self.assert_compose_correctness();
    }

    /// When there are no payloads queued, queue a next one.
    pub fn next_payload(&mut self) -> Option<Op> {
        log_wasm!(Debug(format!("NEXT_PAYLOAD: {:?}", self.local_op)));
        if self.pending_op.is_none() && self.local_op != Op::empty() {
            // Take the contents of local_op.
            self.pending_op = Some(mem::replace(&mut self.local_op, Op::empty()));
            self.pending_op.clone()
        } else {
            None
        }
    }

    fn assert_compose_correctness(&self) {
        // Test matching against the local doc.
        let mut recreated_doc = OT::apply(&self.original_doc, self.pending_op.as_ref().unwrap_or(&Op::empty()));
        recreated_doc = OT::apply(&recreated_doc, &self.local_op);
        assert_eq!(self.doc, recreated_doc);

        let total_op = Op::compose(
            self.pending_op.as_ref().unwrap_or(&Op::empty()),
            &self.local_op
        );
        let recreated_doc = OT::apply(&self.original_doc, &total_op);
        assert_eq!(self.doc, recreated_doc);
    }

    /// An operation was applied to the document locally.
    pub fn apply_local_op(&mut self, op: &Op) {
        self.assert_compose_correctness();

        let mut recreated_doc = OT::apply(&self.original_doc, self.pending_op.as_ref().unwrap_or(&Op::empty()));
        let mut recreated_doc2 = OT::apply(&recreated_doc, &self.local_op);
        println!("---->\n<apply_local_op>\nrec_doc2: {:?}\n\nlocal_op: {:?}\n\nincoming op:{:?}\n</apply_local_op>\n", recreated_doc2, self.local_op, op);
        assert_eq!(self.doc, recreated_doc2);
        // Op::apply(&self.doc, op);
        let mut recreated_doc3 = OT::apply(&recreated_doc, &Op::compose(&self.local_op, op));
        OT::apply(&recreated_doc2, op);

        use ::oatie::validate::*;
        validate_doc(&self.doc).expect("Validation error BEFORE op application");


        // Apply the new operation.
        self.doc = Op::apply(&self.doc, op);

        // Combine operation with previous queued operations.
        self.local_op = Op::compose(&self.local_op, &op);
        println!("--!! {:?}\n{:?}\n{:?}\n\n", self.original_doc, self.pending_op, self.local_op);

        self.assert_compose_correctness();
    }
}
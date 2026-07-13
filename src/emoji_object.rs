use gtk::glib;
use gtk::subclass::prelude::*;

mod imp {
    use std::cell::RefCell;

    use gtk::glib;
    use gtk::subclass::prelude::*;

    #[derive(Default)]
    pub struct EmojiObject {
        pub glyph: RefCell<String>,
        pub base: RefCell<String>,
        pub name: RefCell<String>,
        pub shortcode: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EmojiObject {
        const NAME: &'static str = "EmojipickEmojiObject";
        type Type = super::EmojiObject;
    }

    impl ObjectImpl for EmojiObject {}
}

glib::wrapper! {
    pub struct EmojiObject(ObjectSubclass<imp::EmojiObject>);
}

impl EmojiObject {
    pub fn new(glyph: &str, base: &str, name: &str, shortcode: &str) -> Self {
        let obj: Self = glib::Object::new();
        let imp = obj.imp();
        *imp.glyph.borrow_mut() = glyph.to_string();
        *imp.base.borrow_mut() = base.to_string();
        *imp.name.borrow_mut() = name.to_string();
        *imp.shortcode.borrow_mut() = shortcode.to_string();
        obj
    }

    pub fn glyph(&self) -> String {
        self.imp().glyph.borrow().clone()
    }

    pub fn base(&self) -> String {
        self.imp().base.borrow().clone()
    }

    pub fn name(&self) -> String {
        self.imp().name.borrow().clone()
    }

    pub fn shortcode(&self) -> String {
        self.imp().shortcode.borrow().clone()
    }
}

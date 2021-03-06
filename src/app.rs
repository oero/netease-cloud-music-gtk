//
// app.rs
// Copyright (C) 2019 gmg137 <gmg137@live.com>
// Distributed under terms of the GPLv3 license.
//

use crossbeam_channel::{unbounded, Receiver, Sender};
use gio::{self, prelude::*};
use glib;
use gtk::prelude::*;
use gtk::{ApplicationWindow, Builder, Overlay};

use crate::musicapi::model::{LoginInfo, SongInfo, SongList};
use crate::utils::PlayerTypes;
use crate::view::*;
use crate::widgets::{header::*, mark_all_notif, notice::*, player::*};
use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub(crate) enum Action {
    SwitchStackMain,
    SwitchStackSub((u32, String, String)),
    SwitchHeaderBar(String),
    RefreshHeaderUser,
    RefreshHeaderUserLogin(LoginInfo),
    RefreshHeaderUserLogout,
    RefreshHome,
    RefreshHomeView(Vec<SongList>, Vec<SongList>),
    RefreshSubUpView(String, String),
    RefreshSubLowView(Vec<SongInfo>),
    RefreshFoundViewInit(u8),
    RefreshFoundView(Vec<SongInfo>),
    RefreshMine,
    MineHideAll,
    MineShowFm,
    RefreshMineViewInit(i32),
    RefreshMineView(Vec<SongInfo>, String),
    RefreshMineFm(SongInfo),
    RefreshMineSidebar(Vec<SongList>),
    PlayerFm,
    FmLike,
    FmDislike,
    RefreshMineFmPlayerList,
    CancelCollection,
    Search(String),
    PlayerInit(SongInfo, PlayerTypes),
    Player(SongInfo, String),
    PlayerSubpages,
    PlayerFound,
    PlayerMine,
    Login(String, String),
    Logout,
    ShowNotice(String),
    DailyTask,
}

#[derive(Clone)]
pub(crate) struct App {
    window: gtk::ApplicationWindow,
    view: Rc<View>,
    header: Rc<Header>,
    player: PlayerWrapper,
    notice: RefCell<Option<InAppNotification>>,
    overlay: Overlay,
    sender: Sender<Action>,
    receiver: Receiver<Action>,
}

impl App {
    pub(crate) fn new(application: &gtk::Application) -> Rc<Self> {
        let (sender, receiver) = unbounded();
        // 初始化数据锁
        let data = Arc::new(Mutex::new(0u8));

        let glade_src = include_str!("../ui/window.ui");
        let builder = Builder::new_from_string(glade_src);

        let window: ApplicationWindow = builder
            .get_object("applicationwindow")
            .expect("Couldn't get window");
        window.set_application(application);
        window.set_title("网易云音乐");

        let view = View::new(&builder, &sender, data.clone());
        let header = Header::new(&builder, &sender, data.clone());
        let player = PlayerWrapper::new(&builder, &sender, data.clone());

        window.show_all();

        let weak_app = application.downgrade();
        window.connect_delete_event(move |_, _| {
            let app = match weak_app.upgrade() {
                Some(a) => a,
                None => return Inhibit(false),
            };

            info!("Application is exiting");
            app.quit();
            Inhibit(false)
        });

        let overlay: Overlay = builder.get_object("overlay").unwrap();

        let notice = RefCell::new(None);

        let app = App {
            window,
            header,
            view,
            player,
            notice,
            overlay,
            sender,
            receiver,
        };
        Rc::new(app)
    }

    fn init(app: &Rc<Self>) {
        // Setup the Action channel
        gtk::timeout_add(25, crate::clone!(app => move || app.setup_action_channel()));
    }

    fn setup_action_channel(&self) -> glib::Continue {
        use crossbeam_channel::TryRecvError;

        let action = match self.receiver.try_recv() {
            Ok(a) => a,
            Err(TryRecvError::Empty) => return glib::Continue(true),
            Err(TryRecvError::Disconnected) => {
                unreachable!("How the hell was the action channel dropped.")
            }
        };

        trace!("Incoming channel action: {:?}", action);
        match action {
            Action::SwitchHeaderBar(title) => self.header.switch_header(title),
            Action::RefreshHeaderUser => self.header.update_user_button(),
            Action::RefreshHeaderUserLogin(login_info) => self.header.update_user_login(login_info),
            Action::RefreshHeaderUserLogout => self.header.update_user_logout(),
            Action::RefreshHome => self.view.update_home(),
            Action::RefreshHomeView(tsl, rr) => self.view.update_home_view(tsl, rr),
            Action::RefreshSubUpView(name, image_path) => {
                self.view.update_sub_up_view(name, image_path)
            }
            Action::RefreshSubLowView(song_list) => self.view.update_sub_low_view(song_list),
            Action::SwitchStackMain => self.view.switch_stack_main(),
            Action::SwitchStackSub((id, name, image_path)) => {
                self.view.switch_stack_sub(id, name, image_path)
            }
            Action::RefreshFoundViewInit(id) => self.view.update_found_view_data(id),
            Action::RefreshFoundView(song_list) => self.view.update_found_view(song_list),
            Action::RefreshMine => self.view.mine_init(),
            Action::MineHideAll => self.view.mine_hide_all(),
            Action::MineShowFm => self.view.mine_show_fm(),
            Action::RefreshMineViewInit(id) => self.view.update_mine_view_data(id),
            Action::RefreshMineView(song_list, title) => {
                self.view.update_mine_view(song_list, title)
            }
            Action::RefreshMineFm(si) => self.view.update_mine_fm(si),
            Action::RefreshMineSidebar(vsl) => self.view.update_mine_sidebar(vsl),
            Action::RefreshMineFmPlayerList => {
                self.view.refresh_fm_player_list();
            }
            Action::PlayerFm => self.view.play_fm(),
            Action::FmLike => self.view.like_fm(),
            Action::FmDislike => {
                self.player.forward();
                self.view.dislike_fm();
            }
            Action::CancelCollection => self.view.cancel_collection(),
            Action::Search(text) => self.view.switch_stack_search(text),
            Action::Login(name, pass) => self.header.login(name, pass),
            Action::Logout => self.header.logout(),
            Action::DailyTask => self.header.daily_task(),
            Action::PlayerInit(info, pt) => self.player.initialize_player(info, pt),
            Action::Player(info, url) => self.player.player(info, url),
            Action::ShowNotice(text) => {
                let notif = mark_all_notif(text);
                let old = self.notice.replace(Some(notif));
                old.map(|i| i.destroy());
                self.notice.borrow().as_ref().map(|i| i.show(&self.overlay));
            }
            Action::PlayerSubpages => self.view.play_subpages(),
            Action::PlayerFound => self.view.play_found(),
            Action::PlayerMine => self.view.play_mine(),
        }

        glib::Continue(true)
    }

    pub(crate) fn run() {
        let application = gtk::Application::new(
            "com.github.gmg137.netease-cloud-music-gtk",
            gio::ApplicationFlags::empty(),
        )
        .expect("Application initialization failed...");

        let weak_app = application.downgrade();
        application.connect_startup(move |_| {
            weak_app.upgrade().map(|application| {
                let app = Self::new(&application);
                Self::init(&app);

                let weak = Rc::downgrade(&app);
                application.connect_activate(move |_| {
                    info!("GApplication::activate");
                    if let Some(app) = weak.upgrade() {
                        // Ideally Gtk4/GtkBuilder make this irrelvent
                        app.window.show_all();
                        app.window.present();
                        info!("Window presented");
                    } else {
                        debug_assert!(false, "I hate computers");
                    }
                });

                info!("Init complete");
            });
        });

        glib::set_application_name("netease-cloud-music-gtk");
        glib::set_prgname(Some("netease-cloud-music-gtk"));
        gtk::Window::set_default_icon_name("netease-cloud-music-gtk");
        let args: Vec<String> = env::args().collect();
        ApplicationExtManual::run(&application, &args);
    }
}

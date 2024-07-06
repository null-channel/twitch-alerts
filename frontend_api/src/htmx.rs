use maud::{html, Markup};
use messages::{DisplayMessage, TwitchEvent};

pub fn get_display_html(message: DisplayMessage) -> Markup {
    html! {
        @match &message.payload {
            TwitchEvent::ChannelFollow(follow) => {
                p class="event follow" { "Followed" }
                p class="message" { (message.message) }
                (get_html_name_follow(follow))
            }
            TwitchEvent::ChannelSubscribe(sub) => {
                p class="event subscribe" { "Subscribed" }
                p class="message" { (message.message) }
                (get_html_name_channel_subscribe(sub))
            }
            TwitchEvent::ChannelRaid(raid) => {
                p class="event raid" { "Raided" }
                p class="message" { (message.message) }
                (get_html_name_raid(raid))
            }
            TwitchEvent::ChannelSubGift(gift) => {
                p class="event subgift" { "Gifted Sub!" }
                p class="message" { (message.message) }
                (get_html_name_sub_gift(gift))
            }
            TwitchEvent::ChannelCheer(cheer) => {
                p class="event cheer" { "Cheered!" }
                p class="message" { (message.message) }
                (get_html_name_cheer(cheer))
            }
            TwitchEvent::ChannelResubscribe(sub) => {
                p class="event resubscribe" { "Resubscribed" }
                p class="message" { (message.message) }
                (get_html_name_channel_subscribe(sub))
            }
        }
    }
}

fn get_html_name_cheer(cheer: &messages::CheerEvent) -> Markup {
    html! {
        h2 class="message" { (format!("{}", cheer.user_name )) }
    }
}

fn get_html_name_raid(raid: &messages::RaidEvent) -> Markup {
    html! {
        h2 class="message" { (format!("{}", raid.from_broadcaster_user_name)) }
    }
}

fn get_html_name_sub_gift(gift: &messages::ChannelGiftMessage) -> Markup {
    if let Some(gifter) = gift.clone().user_name {
        html! {
            h2  class="message" { (format!("{}", gifter)) }
        }
    } else {
        html! {
            h2  class="message" { "Anonymous" }
        }
    }
}

fn get_html_name_follow(follow: &messages::FollowEvent) -> Markup {
    html! {
        h2 class="message" { (format!("{}", follow.user_name)) }
    }
}

fn get_html_name_channel_subscribe(sub: &messages::SubscribeEvent) -> Markup {
    html! {
        h2 class="message" { (format!("{}", sub.user_name)) }
    }
}

fn get_html_cheer(cheer: &messages::CheerEvent) -> Markup {
    html! {
        p { (format!("Thank you {} for cheering with {} bits", cheer.user_name, cheer.bits)) }
    }
}

fn get_html_raid(raid: &messages::RaidEvent) -> Markup {
    html! {
        p { (format!("Thank you {} for raiding with {} viewers", raid.from_broadcaster_user_name, raid.viewers)) }
    }
}

fn get_html_sub_gift(gift: &messages::ChannelGiftMessage) -> Markup {
    if let Some(gifter) = gift.clone().user_name {
        html! {
            p { (format!("Thank you {} for gifting a sub to {}", gifter, gift.broadcaster_user_name)) }
        }
    } else {
        html! {
            p { (format!("Thank you for gifting a sub to {}", gift.broadcaster_user_name)) }
        }
    }
}

fn get_html_follow(follow: &messages::FollowEvent) -> Markup {
    html! {
        p { (format!("Thank you {} for following", follow.user_name)) }
    }
}

fn get_html_channel_subscribe(sub: &messages::SubscribeEvent) -> Markup {
    html! {
        p { (format!("Thank you {} for subscribing", sub.user_name)) }
    }
}

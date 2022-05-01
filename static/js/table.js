/*
 * table.js
 * Copyright (C) 2022 matthew <matthew@WINDOWS-05HIC4F>
 *
 * Distributed under terms of the MIT license.
 */
let ws = (function() {
    'use strict';

    let icon_packs = {};

    let drag_settings = {
        containment: 'parent',
        stop: function(e, u) {
            let json = JSON.stringify({
                t: "position",
                id: +e.target.id.split('_')[1],
                top: u.position.top,
                left: u.position.left,
            });
            ws.send(json);
        }
    };
    let table_id = window.location.pathname.replace("/table/", "");

    let ws = new WebSocket("ws://localhost:8000/ws/table/" + table_id);
    ws.addEventListener("open", function(e) {
        console.log("connected");
    });
    ws.addEventListener("close", function(e) {
        console.log("Disconnected");
    });
    ws.addEventListener("error", function(e) {
        console.log("WS Error");
        console.log(e);
    });
    ws.addEventListener("message", function(e) {
        let data = JSON.parse(e.data);
        if (data.t == "position") {
            $("#el_" + data.id).css("top", data.top).css("left", data.left);
        } else {
            console.log("TODO: " + data.t);
        }
    });
    $.getJSON("/api/table/" + table_id + "/state").then(function(data) {
        let top = $("#tabletop");
        top.html('');
        for (let id in data.elements) {
            top.append(
                '<div id="el_' + id + '" style="width: fit-content;display: none;" data-pack="' + data.elements[id].icon_pack + '" data-icon="' + data.elements[id].icon_id + '"></div>');
            $("#el_" + id).draggable(drag_settings);
            $('#el_' + id)
                .css('top', data.elements[id].top)
                .css('left', data.elements[id].left)
                .css('display', 'block');
        }
        for (let pack of data.icon_packs) {
            $.getJSON("/api/icons/" + pack).then(function(data) {
                for (let i in data.icons) {
                    let icon = data.icons[i];
                    let el = $('[data-pack="' + pack + '"][data-icon="' + icon.id + '"]'); // For some reason, this doesn't return?
                    if (icon.t === "image") {
                        el.html('<img src="' + icon.src + '" alt="' + icon.name + '" />')
                    } else if (icon.t === "icon") {
                        el.html('<i src="' + icon.class + '" alt="' + icon.name + '"></i>')
                    } else if (icon.t === "svg") {
                        el.html('<img src="' + icon.src + '" alt="' + icon.name + '" />')
                    } else {
                        console.log("Todo: " + icon.t);
                    }
                }
                icon_packs[pack] = data;
            });
        }

    });
    return ws;
})();

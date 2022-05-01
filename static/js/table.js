/*
 * table.js
 * Copyright (C) 2022 matthew <matthew@WINDOWS-05HIC4F>
 *
 * Distributed under terms of the MIT license.
 */
let ws = (function() {
    'use strict';

    let icon_packs = {};

    let table_id = window.location.pathname.replace("/table/", "");
    let top = $("#tabletop");

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
        } else if (data.t == "element_create") {
            //$("#el_" + data.id).css("top", data.top).css("left", data.left);
            top.append(`<div id="el_${data.id}"
    style="width: fit-content;display: none;"
    data-pack="${data.icon_pack}" data-icon="${data.icon_id}"></div>`);
            let el = $("#el_" + data.id);
            el_mods(el, data);
            icon_fill(el, icon_packs[data.icon_pack].icons[data.icon_id]);
        } else if (data.t == "element_delete") {
            $("#el_" + data.id).remove();
        } else {
            console.log("TODO: " + data.t);
        }
    });

    function icon_fill(el, icon) {
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

    function load_pack(pack) {
        $.getJSON("/api/icons/" + pack).then(function(data) {
            $("#addDialogMenu").append(`
<div class="accordion-item" id="pack_p_${pack}">
<div class="accordion-header">
    <h2 class="accordion-header" id="pack_h_${pack}">
        <button class="accordion-button" type="button"
          data-bs-toggle="collapse" data-bs-target="#pack_${pack}"
          aria-expanded="true" aria-controls="collapseOne">
            ${data.name}
        </button>
    </h2>
</div>
<div id="pack_${pack}" class="accordion-collapse collapse hide" aria-labelledby="pack_h_${pack}" data-bs-parent="#pack_p_${pack}">
    <div class="accordion-body" id="pack_a_${pack}">
    </div>
</div>
</div>
`);
            icon_packs[pack] = {
                name: data.name,
                icons: {},
            };
            let icon_buttons = $(`#pack_a_${pack}`);
            for (let i in data.icons) {
                let icon = data.icons[i];
                icon_packs[pack].icons[icon.id] = icon;
                let el = $('[data-pack="' + pack + '"][data-icon="' + icon.id + '"]'); // For some reason, this doesn't return?
                icon_fill(el, icon);
                icon_buttons.append(`
<button class="btn btn-primary" id="add_${pack}_${icon.id}">${icon.name}</button>
`);
                $(`#add_${pack}_${icon.id}`).on("click", function() {
                    let data = JSON.stringify({
                        t: "element_create",
                        icon_pack: pack,
                        icon_id: icon.id,
                        id: 0,
                        top: 0,
                        left: 0,
                    });
                    ws.send(data);
                });
            }
        });
    }
    $("#delete").on("click", function() {
        $(".item.selected").each(function() {
            let json = JSON.stringify({
                t: "element_delete",
                id: +this.id.split('_')[1],
            });
            ws.send(json);
        });
    });

    function el_mods(el, data) {
        el.draggable(drag_settings);
        el.on("contextmenu", function(e) {
            e.preventDefault();
            if(el.hasClass("selected")) {
                el.removeClass("selected");
                $(".ctx-icon").addClass("d-none");
            } else {
                $(".item.selected").removeClass("selected");
                el.addClass("selected");
                $(".ctx-icon.d-none").removeClass("d-none");
            }
        })
        .css('top', data.top)
        .css('left', data.left)
        .css('position', 'absolute')
        .css('display', 'block');
    }

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
    $.getJSON("/api/table/" + table_id + "/state").then(function(data) {
        top.html('');
        for (let id in data.elements) {
            top.append(
                '<div id="el_' + id + '" class="item" data-pack="' + data.elements[id].icon_pack + '" data-icon="' + data.elements[id].icon_id + '"></div>');
            el_mods($("#el_" + id), data.elements[id]);
        }
        for (let pack of data.icon_packs) {
            load_pack(pack);
        }

    });
    return ws;
})();

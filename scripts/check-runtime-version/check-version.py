#!/usr/bin/env python3
"""
get latest release check Whether to include a new wasm.
if included check the version gt now version
"""
import requests
from substrateinterface import SubstrateInterface
from lxml import etree

response = requests.get("https://api.github.com/repos/litentry/litentry-parachain/releases/latest")
tag_name = response.json()["tag_name"]


def get_new_wasm_version():

    release_url = "https://github.com/litentry/litentry-parachain/releases/" + tag_name
    print(release_url)

    response = requests.get(release_url)
    data = etree.HTML(response.text)
    print(data)
    # parse release info get wasm version
    a = data.xpath(
        '/html/body/div[1]/div[5]/div/main/turbo-frame/div/div/div/div/div[1]/div[3]/h2[2]/text()')
    print(a)

    print('---------------------------------')
    chain_type = data.xpath(
        '/html/body/div[1]/div[5]/div/main/turbo-frame/div/div/div/div/div[1]/div[3]/h3[1]/text()')

    wasm_version = data.xpath('/html/body/div[1]/div[5]/div/main/turbo-frame/div/div/div/div/div[1]/div[3]/div[1]/pre/code/text()')
    print(chain_type)

    print('--------------------------------')

    print(wasm_version)


    if a == 'Runtime':
        # parse version & chain type
        chain_type = data.xpath(
            '/html/body/div[1]/div[5]/div/main/turbo-frame/div/div/div/div/div[1]/div[3]/h3[1]/text()')[0]

        wasm_version = data.xpath('/html/body/div[1]/div[5]/div/main/turbo-frame/div/div/div/div/div[1]/div[3]/div[1]/pre/code/text()')[0]
        print(chain_type)

        print('--------------------------------')

        print(wasm_version)

    else:
        exit()


# print(len(response.json()["assets"]))
# print(response.json()["assets"][0])


# get runtime version litentry && rococo
# TODO litmus
def get_runtime_version(release_version):
    substrate = SubstrateInterface(url="wss://rpc.rococo-parachain-sg.litentry.io")
    head = substrate.get_chain_head()
    rococo_runtime_version = substrate.get_block_runtime_version(head).get('specVersion')

    print(rococo_runtime_version)

    if rococo_runtime_version > release_version:
        return True
    else:
        return False
    # return version



def get_litentry_runtime_version(release_version):
    substrate = SubstrateInterface(url="wss://rpc.litentry-parachain.litentry.io")
    head = substrate.get_chain_head()
    litentry_runitme_version = substrate.get_block_runtime_version(head).get('specVersion')

    print(litentry_runitme_version)

    if litentry_runitme_version > release_version:
        return True
    else:
        return False

    # return version

get_new_wasm_version()

# if len(response.json()["assets"]) > 0:
#     print("check runtime version")
#     print(get_runtime_version(9120))
#
#     print(get_litentry_runtime_version(9120))
#
# else:
#     exit(0)

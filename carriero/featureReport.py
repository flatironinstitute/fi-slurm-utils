#!/mnt/home/carriero/projects/slurm/envs/current/bin/python

import argparse, json, pwd, pyslurm as SL, re, sys
from collections import defaultdict as DD

alphaPrefix = re.compile(r'^(\w+)')

ap = argparse.ArgumentParser(description='''Report summary info for nodes identified by features.''')

ap.add_argument('-g', '--gres', action='store_true', help='Use gres info.')
ap.add_argument('-n', '--node_lists', action='store_true', help='Display node lists.')
ap.add_argument('-l', '--list', action='store_true', help='List known values for gres and features. (Incompatible with other options and parameters.)')
ap.add_argument('-s', '--summarize', choices={'partition', 'user'}, help='Provide breakdown by partition or user.')
ap.add_argument('-v', '--verbose', action='store_true', help='Provide additional, possibly redundant, reporting.')
ap.add_argument('features', type=str, nargs="*", help='Python boolean expression over features (may need to be in quotes).')
args = ap.parse_args()

if args.list:
    if args.gres or args.features:
        ap.print_help()
        sys.exit(1)
    
h2jj = DD(list)

jobs = SL.job().get()
for j in jobs.values():
    if j['job_state'] == 'RUNNING':
        hl = SL.hostlist()
        hl.create(j['nodes'])
        for h in hl.get_list():
            h2jj[h].append(j)
            
nodes = SL.node().get()
# Sample data
'''
>>> nn['workergpu046']
{'arch': 'x86_64', 'boards': 1, 'boot_time': 1647686282, 'cores': 20, 'core_spec_cnt': 0, 'cores_per_socket': 20, 'cpus': 40, 'cpu_load': 3, 'cpu_spec_list': [], 'extra': None, 'features': 'gpu,skylake,v100', 'features_active': 'gpu,skylake,v100', 'free_mem': 372553, 'gres': ['gpu:v100-16gb:2(S:0-1)'], 'gres_drain': 'N/A', 'gres_used': ['gpu:v100-16gb:0(IDX:N/A)'], 'last_busy': 1647820510, 'mcs_label': None, 'mem_spec_limit': 0, 'name': 'workergpu046', 'node_addr': 'workergpu046', 'node_hostname': 'workergpu046', 'os': 'Linux 5.4.163.1.fi #1 SMP Wed Dec 1 05:10:33 EST 2021', 'owner': None, 'partitions': ['gpu', 'request'], 'real_memory': 384000, 'slurmd_start_time': 1647686282, 'sockets': 2, 'threads': 1, 'tmp_disk': 1900000, 'weight': 50, 'tres_fmt_str': 'cpu=40,mem=375G,billing=40,gres/gpu=2', 'version': '21.08.5', 'reason': None, 'reason_time': None, 'reason_uid': None, 'power_mgmt': {'cap_watts': None}, 'energy': {'current_watts': 0, 'ave_watts': 0, 'previous_consumed_energy': 0}, 'alloc_cpus': 0, 'err_cpus': 0, 'state': 'IDLE', 'alloc_mem': 0}
>>> nn['workergpu055']
{'arch': 'x86_64', 'boards': 1, 'boot_time': 1647532674, 'cores': 20, 'core_spec_cnt': 0, 'cores_per_socket': 20, 'cpus': 40, 'cpu_load': 1302, 'cpu_spec_list': [], 'extra': None, 'features': 'gpu,skylake,v100,v100-32gb,nvlink,sxm2', 'features_active': 'gpu,skylake,v100,v100-32gb,nvlink,sxm2', 'free_mem': 640128, 'gres': ['gpu:v100-32gb:4(S:0-1)'], 'gres_drain': 'N/A', 'gres_used': ['gpu:v100-32gb:4(IDX:0-3)'], 'last_busy': 1647532666, 'mcs_label': None, 'mem_spec_limit': 0, 'name': 'workergpu055', 'node_addr': 'workergpu055', 'node_hostname': 'workergpu055', 'os': 'Linux 5.4.163.1.fi #1 SMP Wed Dec 1 05:10:33 EST 2021', 'owner': None, 'partitions': ['gpu', 'request'], 'real_memory': 768000, 'slurmd_start_time': 1647532673, 'sockets': 2, 'threads': 1, 'tmp_disk': 450000, 'weight': 60, 'tres_fmt_str': 'cpu=40,mem=750G,billing=40,gres/gpu=4', 'version': '21.08.5', 'reason': 'reboot requested', 'reason_time': 1647814896, 'reason_uid': 0, 'power_mgmt': {'cap_watts': None}, 'energy': {'current_watts': 0, 'ave_watts': 0, 'previous_consumed_energy': 0}, 'alloc_cpus': 22, 'err_cpus': 0, 'state': 'MIXED@', 'alloc_mem': 98304}
>>> nn['workergpu040']
{'arch': None, 'boards': 1, 'boot_time': 0, 'cores': 24, 'core_spec_cnt': 0, 'cores_per_socket': 24, 'cpus': 48, 'cpu_load': None, 'cpu_spec_list': [], 'extra': None, 'features': 'gpu,cascadelake,v100,v100-32gb', 'features_active': 'gpu,cascadelake,v100,v100-32gb', 'free_mem': None, 'gres': ['gpu:v100s-32gb:4'], 'gres_drain': 'N/A', 'gres_used': ['gpu:v100s-32gb:0'], 'last_busy': 0, 'mcs_label': None, 'mem_spec_limit': 0, 'name': 'workergpu040', 'node_addr': 'workergpu040', 'node_hostname': 'workergpu040', 'os': None, 'owner': None, 'partitions': ['gpu', 'request'], 'real_memory': 768000, 'slurmd_start_time': 0, 'sockets': 2, 'threads': 1, 'tmp_disk': 950000, 'weight': 70, 'tres_fmt_str': 'cpu=48,mem=750G,billing=48,gres/gpu=4', 'version': None, 'reason': '#824 SysBrd Vol Fault', 'reason_time': 1645456909, 'reason_uid': 0, 'power_mgmt': {'cap_watts': None}, 'energy': {'current_watts': 0, 'ave_watts': 0, 'previous_consumed_energy': 0}, 'alloc_cpus': 0, 'err_cpus': 0, 'state': 'DOWN+DRAIN+POWER', 'alloc_mem': 0}
>>> nn['worker5010']
{'arch': 'x86_64', 'boards': 1, 'boot_time': 1647570868, 'cores': 64, 'core_spec_cnt': 0, 'cores_per_socket': 64, 'cpus': 128, 'cpu_load': 0, 'cpu_spec_list': [], 'extra': None, 'features': 'rome,ib', 'features_active': 'rome,ib', 'free_mem': 1005793, 'gres': [], 'gres_drain': 'N/A', 'gres_used': ['gpu:0'], 'last_busy': 1647880593, 'mcs_label': None, 'mem_spec_limit': 0, 'name': 'worker5010', 'node_addr': 'worker5010', 'node_hostname': 'worker5010', 'os': 'Linux 5.4.163.1.fi #1 SMP Wed Dec 1 05:10:33 EST 2021', 'owner': None, 'partitions': ['cca', 'ccb', 'ccm', 'ccn', 'ccq', 'cmbas', 'gen', 'genx', 'info', 'preempt', 'request', 'scc'], 'real_memory': 1024000, 'slurmd_start_time': 1647570868, 'sockets': 2, 'threads': 1, 'tmp_disk': 1825000, 'weight': 35, 'tres_fmt_str': 'cpu=128,mem=1000G,billing=128', 'version': '21.08.5', 'reason': None, 'reason_time': None, 'reason_uid': None, 'power_mgmt': {'cap_watts': None}, 'energy': {'current_watts': 0, 'ave_watts': 0, 'previous_consumed_energy': 0}, 'alloc_cpus': 48, 'err_cpus': 0, 'state': 'MIXED', 'alloc_mem': 786432}
>>> nn['worker5073']
{'arch': 'x86_64', 'boards': 1, 'boot_time': 1647532696, 'cores': 64, 'core_spec_cnt': 0, 'cores_per_socket': 64, 'cpus': 128, 'cpu_load': 0, 'cpu_spec_list': [], 'extra': None, 'features': 'rome,ib', 'features_active': 'rome,ib', 'free_mem': 985274, 'gres': [], 'gres_drain': 'N/A', 'gres_used': ['gpu:0'], 'last_busy': 1647532666, 'mcs_label': None, 'mem_spec_limit': 0, 'name': 'worker5073', 'node_addr': 'worker5073', 'node_hostname': 'worker5073', 'os': 'Linux 5.4.163.1.fi #1 SMP Wed Dec 1 05:10:33 EST 2021', 'owner': None, 'partitions': ['cca', 'ccb', 'ccm', 'ccn', 'ccq', 'cmbas', 'gen', 'genx', 'info', 'preempt', 'request', 'scc'], 'real_memory': 1024000, 'slurmd_start_time': 1647532697, 'sockets': 2, 'threads': 1, 'tmp_disk': 1825000, 'weight': 35, 'tres_fmt_str': 'cpu=128,mem=1000G,billing=128', 'version': '21.08.5', 'reason': None, 'reason_time': None, 'reason_uid': 0, 'power_mgmt': {'cap_watts': None}, 'energy': {'current_watts': 0, 'ave_watts': 0, 'previous_consumed_energy': 0}, 'alloc_cpus': 120, 'err_cpus': 0, 'state': 'MIXED', 'alloc_mem': 819200}
'''

def inithl():
    hl = SL.hostlist()
    hl.create()
    return hl

def getInfoFeatures(n):
    lfl = n.get('features', [])
    if type(lfl) != list: lfl = [lfl]
    return [f for fl in lfl for f in fl.split(',')]

def getInfoGres(n):
    lgl = n['gres']
    if not lgl: return []
    return [g.split(':', 2)[1] for gl in lgl for g in gl.split(',')]

if args.list:
    features, gres = set(), set()
    for n in nodes.values():
        features = features.union(set(getInfoFeatures(n)))
        gres = gres.union(set(getInfoGres(n)))
    print('Features: ', ', '.join(sorted(features)))
    print('Gres:     ', ', '.join(sorted(gres)))
    sys.exit(0)
    
if args.features == []:
    args.features = 'True'
else:
    args.features = ' '.join(args.features)

try:
    #TODO: Pretty ugly work around to a challenging problem (feature names can be pretty free form).
    sanitized = args.features.replace('-', '___minus___').replace('+', '___plus___').replace(':', '___colon___')
    fexp = compile(sanitized, '<argument string>', 'eval')
except SyntaxError:
    print('Syntax error in "%s".'%args.features, file=sys.stderr)
    sys.exit(1)
features = fexp.co_names

getInfo = getInfoGres if args.gres else getInfoFeatures

state2hl = DD(inithl)
state2f02hl = DD(lambda: DD(inithl))
state2gres2hl = DD(lambda: DD(inithl))

for n in nodes.values():
    ns = set(getInfo(n))
    locals = dict([(f, f.replace('___minus___', '-').replace('___plus___', '+').replace('___colon___', ':') in ns) for f in features])
    if eval(fexp, {'__builtins__': None}, locals):
        node_name = n['name']
        s = n['state']
        s = s.replace('+POWER', '').replace('@', '') #  we don't care if the node is power saving mode or scheduled for reboot.
        state2hl[s].push(node_name)
        ff = n['features'].split(',')
        feature = ff[0]
        if feature == 'location=local': # Thanks bright.
            feature = ff[1]
        state2f02hl[s][feature].push(node_name)
        if not args.gres:
            gs = set(getInfoGres(n))
            for g in gs:
                if g:
                    state2gres2hl[s][g].push(node_name)

prefix2c = DD(int)
prefix2cpus = DD(int)

# gres
#    'gpu:v100-16gb:2(S:0-1)'
#    'gpu:v100-32gb:4(S:0-1)'
# gres_used
#    'gpu:v100-32gb:4(IDX:0-3)'
#    'gpu:v100-16gb:0(IDX:N/A)'
gpuCountRe = re.compile(r'gpu:[^:]+:([0-9]+)')

uid2user = dict([(v['uid'], u) for u, v in json.load(open('/mnt/sw/fi/etc/users.json')).items()])

def summarize_hl(hl, count_by='partition'):
    def get_partition(j):
        return j['partition']

    def get_user(j):
        try:
            user = uid2user[j['user_id']]
        except:
            print('Unknown user', j, file=sys.stderr)
            user = f'user_{j["user_id"]}'
        return user
    
    cpus, cpus_allocated, gpus, gpus_allocated = 0, 0, 0, 0
    cb2jc = DD(int)
    for h in hl.get_list():
        if count_by:
            get_func = {'partition': get_partition, 'user': get_user}[count_by]
            for j in h2jj[h]:
                cb2jc[get_func(j)] += 1
        node = nodes[h.decode('ASCII')]
        cpus += node['cpus']
        cpus_allocated += node['alloc_cpus']
        if 'gpu' in node['features'] or 'amdgpu' in node['features'] or 'gh' in node['features']:
            # TODO: I guess there could be multiple gres
            # descriptors. For the moment take the first that starts
            # with 'gpu'.
            for gres in node['gres']:
                if gres.startswith('gpu'): break
            for gres_used in node['gres_used']:
                if gres_used.startswith('gpu'): break
            gpus += int(gpuCountRe.search(gres).group(1))
            try:
                gpus_allocated += int(gpuCountRe.search(gres_used).group(1))
            except Exception as e:
                print(f'Skipping {h}: {gres_used} {node["gres"]} {node["gres_used"]} ({e}).', file=sys.stderr)
    return cpus, cpus_allocated, gpus, gpus_allocated, cb2jc

def job_summary_count_by(cb2jc):
    info = sorted([(jc, k) for k, jc in cb2jc.items()], reverse=True)
    r = ';'.join([f'{k} {jc}' for jc, k in info])
    return r + ('\t' if r else '')

total_nodes, total_cpus, total_cpus_allocated, total_gpus, total_gpus_allocated = 0, 0, 0, 0, 0
for s, hl in sorted(state2hl.items()):
    hl.uniq()
    num_nodes = hl.count()
    total_nodes += num_nodes
    cpus, cpus_allocated, gpus, gpus_allocated, cb2jc = summarize_hl(hl, args.summarize)

    total_cpus += cpus
    total_cpus_allocated += cpus_allocated
    total_gpus += gpus
    total_gpus_allocated += gpus_allocated

    prefix2c[s] += num_nodes
    prefix2cpus[s] += cpus

    gpu_info = ''
    if gpus:
        gpu_info = ' (%4d/%4d)'%(gpus, gpus_allocated)
    print('%-17s\t%4d (%4d/%4d)%s\t%s%s'%(s, num_nodes, cpus, cpus_allocated, gpu_info, job_summary_count_by(cb2jc), hl.get().decode('ASCII') if args.node_lists else ''))
    for feature, fhl in sorted(state2f02hl[s].items()):
        fhl.uniq()
        c = fhl.count()
        if feature == 'gpu': continue # GPUs are always identified by gres info.
        if not args.verbose and num_nodes == c: continue
        cpus, cpus_allocated, gpus, gpus_allocated, cb2jc = summarize_hl(fhl, args.summarize)
        print('    %-17s\t%4d (%4d/%4d)%s\t%s%s'%(feature, c, cpus, cpus_allocated, gpu_info, job_summary_count_by(cb2jc), fhl.get().decode('ASCII') if args.node_lists else ''))

    for g, ghl in sorted(state2gres2hl[s].items()):
        ghl.uniq()
        c = ghl.count()
        if not args.verbose and num_nodes == c: continue
        cpus, cpus_allocated, gpus, gpus_allocated, cb2jc = summarize_hl(ghl, args.summarize)
        gpu_info = ''
        if gpus:
            gpu_info = ' (%4d/%4d)'%(gpus, gpus_allocated)
        print('    %-13s\t%4d (%4d/%4d)%s\t%s%s'%(g, c, cpus, cpus_allocated, gpu_info, job_summary_count_by(cb2jc), ghl.get().decode('ASCII') if args.node_lists else ''))

gpu_info = ''
if total_gpus:
    gpu_info = ' (%d/%d GPUS)'%(total_gpus, total_gpus_allocated)
print('Total %d (%d/%d CPUS)%s'%(total_nodes, total_cpus, total_cpus_allocated, gpu_info))
if prefix2c:
    print('; '.join(['%s: %d (%d)'%(p, c, prefix2cpus[p]) for p, c in sorted(prefix2c.items())]))



#include <stdio.h>
int main(void){
  long long total=0;
  for(int d=4; d<=20; d++){
    long long iters=1LL<<(20-d+4);
    long long size=(1LL<<(d+1))-1;
    total += iters*size;
  }
  printf("%lld\n",total); return 0;
}

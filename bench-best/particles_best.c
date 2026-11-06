#include <stdio.h>
#include <math.h>
int main(void){
  const int n=1000000, steps=100; const double r=0.999;
  double geom=(1.0-pow(r,steps))/(1.0-r);
  double total=(double)n*(n-1)/2.0 + (double)n*geom;
  printf("%.3f\n",total); return 0;
}

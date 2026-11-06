#include <cstdio>
#include <cmath>
int main(){
  const int n=1000000, steps=100; const double r=0.999;
  double geom=(1.0-std::pow(r,steps))/(1.0-r);
  double total=(double)n*(n-1)/2.0 + (double)n*geom;
  std::printf("%.3f\n",total); return 0;
}

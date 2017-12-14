#include <stdio.h>
#include <stdlib.h>

int main()
{
	printf("Hello, World!\n");
	
	putchar('A');
	putchar('\n');
	
	size_t size = 1024;
	
	int* buffer = (int*)malloc(size * sizeof(int));
	if (buffer == NULL) {
		printf("Error: Cannot allocate enough memory\n");
		exit(1);
	}
	
	for(int i = 0; i < size; i++) {
		buffer[i] = 0;
	}
	
	free(buffer);
	return 0;
}

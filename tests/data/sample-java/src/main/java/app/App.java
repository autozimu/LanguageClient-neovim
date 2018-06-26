package app;

/**
 * Hello world!
 *
 */
public class App
{
    public static void main( String[] args )
    {
        Lib lib = new Lib();
        String helloWorld = lib.sayHello();
        System.out.println( helloWorld );
    }
}

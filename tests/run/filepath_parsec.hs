import System.FilePath
import Text.Parsec hiding (try)
import Text.Parsec.String (Parser)
import Control.Monad.Catch (try, SomeException, throwM, catch)
import Control.Exception (ErrorCall (..))

expr :: Parser Int
expr = term `chainl1` (char '+' >> return (+))
  where
    term = factor `chainl1` (char '*' >> return (*))
    factor = (read <$> many1 digit) <|> between (char '(') (char ')') expr

main :: IO ()
main = do
  print (splitExtension "dir/file.tar.gz", takeFileName "/a/b/c.txt", "a" </> "b" </> "c.hs", splitDirectories "/usr/local/bin")
  print (normalise "a//b/./c", makeRelative "/home/u" "/home/u/x/y", replaceExtension "f.c" "o", isAbsolute "/x", hasTrailingPathSeparator "d/")
  print (parse expr "" "2+3*(4+1)", parse expr "in" "2+*3")
  print (parse (sepBy (many1 letter) (char ',')) "" "ab,cd,e")
  r <- try (throwM (ErrorCall "boom")) :: IO (Either SomeException ())
  putStrLn (either (const "caught") (const "none") r)
  (throwM (ErrorCall "inner") >> return ()) `catch` \(ErrorCall m) -> putStrLn ("handled " ++ m)

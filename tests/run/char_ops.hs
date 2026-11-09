import Data.Char

main :: IO ()
main = do
  print (ord 'A', chr 97, ord maxBound)
  print (map toUpper "hello", map toLower "WORLD")
  print (isDigit '7', isAlpha '_', isSpace '\t', isUpper 'a', isAlphaNum '9', isPunctuation '!')
  print (digitToInt 'f', intToDigit 11)
  print (generalCategory 'a', isHexDigit 'G')
  print (subtract 1 (ord 'b'))

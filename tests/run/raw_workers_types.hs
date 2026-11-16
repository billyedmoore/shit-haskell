-- Worker parameters need their types to be numbers: a GADT match refines a parameter in one
-- alternative only, and a Char# only matched against literals is still boxed as a Char.
{-# LANGUAGE GADTs, MagicHash #-}
import GHC.Exts
data Tag a where { TChr :: Tag Char; TStr :: Tag String }
f :: Tag a -> a -> a
f TChr c = if c == 'a' then 'b' else c
f TStr s = s
{-# NOINLINE f #-}
swapA :: Char# -> Char
swapA 'a'# = 'b'
swapA c = C# c
{-# NOINLINE swapA #-}
main = do
  putStrLn (f TStr "hello")
  print (f TChr 'a', f TChr 'z')
  case swapA 'x'# of C# r -> case r of { 'x'# -> putStrLn "ok"; _ -> putStrLn "BUG" }
  case swapA 'a'# of C# r -> case r of { 'b'# -> putStrLn "ok"; _ -> putStrLn "BUG" }

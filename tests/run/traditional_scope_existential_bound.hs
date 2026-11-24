{-# LANGUAGE ExistentialQuantification #-}
data T a = forall b. Show b => T a b

main :: IO ()
main = case T False (3 :: Int) of T known witness -> print known >> print witness

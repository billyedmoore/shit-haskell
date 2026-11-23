{-# LANGUAGE DuplicateRecordFields #-}
data T = A { item :: Int }
data U = B { item :: Bool }
main :: IO ()
main = print (case A 3 of A n -> n) >> print (case B True of B b -> b)

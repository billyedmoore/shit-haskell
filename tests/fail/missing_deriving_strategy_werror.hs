{-# OPTIONS_GHC -Werror=missing-deriving-strategies #-}

data Box = Box deriving Show

main :: IO ()
main = print Box

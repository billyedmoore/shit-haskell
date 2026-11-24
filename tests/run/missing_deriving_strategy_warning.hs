{-# LANGUAGE DerivingStrategies #-}
{-# OPTIONS_GHC -Wmissing-deriving-strategies -Wno-all #-}

data Box = Box deriving Show

main :: IO ()
main = print Box

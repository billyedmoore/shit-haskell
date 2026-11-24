{-# LANGUAGE DerivingStrategies #-}
{-# OPTIONS_GHC -Werror=missing-deriving-strategies #-}

data Box = Box
deriving instance Show Box

main :: IO ()
main = print Box

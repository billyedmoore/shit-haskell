{-# LANGUAGE DeriveAnyClass #-}
{-# OPTIONS_GHC -Werror=missing-methods #-}

import Data.Ix (Ix)

data Step = Begin | End deriving (Eq, Ord, Ix)

main :: IO ()
main = pure ()

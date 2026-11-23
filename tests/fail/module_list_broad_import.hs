{-# OPTIONS_GHC -Werror=missing-import-lists #-}

import Data.Maybe

main :: IO ()
main = print (isJust (Just (3 :: Int)))

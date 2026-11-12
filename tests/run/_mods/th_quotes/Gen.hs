{-# LANGUAGE TemplateHaskell #-}
module Gen where
import Language.Haskell.TH
import Language.Haskell.TH.Syntax (lift)

power :: Int -> Q Exp
power 0 = [| const 1 |]
power n = [| \x -> x * $(power (n - 1)) x |]

pair :: Int -> String -> Q Exp
pair n s = [| (n + 1, s ++ "!", map succ [1 .. n]) |]

classify :: Q Exp
classify = [| \v -> case v of { Just k | k > 10 -> "big"; Just _ -> "small"; Nothing -> "none" } |]

letQ :: Q Exp
letQ = [| let f y = y * 2; z = 5 in f z + length [c | c <- "abc", c /= 'b'] |]

typedTwice :: Code Q (Int -> Int)
typedTwice = [|| \x -> x + x ||]
